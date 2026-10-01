//! The ledger store: happenstance's SQLite event store at `<git-common-dir>/weighin/`.
//!
//! Every write is conditioned (DCB): raising an item fails if that item was already raised, and
//! every later event for an item fails if the item changed since the writer read it. Two sessions
//! deciding the same item at once therefore cannot both win; the loser re-reads and sees the
//! decision. This is the property the per-session JSONL design needed a projector conflict state
//! for; the store now enforces it at write time.
//!
//! # Journal mode
//!
//! happenstance opens its stores in WAL, and SQLite's own default is DELETE. Both delete files
//! beside the database: WAL removes its `-wal` and `-shm` on close, DELETE removes its journal
//! at every commit. Cowork's connected folders refuse deletes (`SQLITE_IOERR_DELETE`, 2570) and
//! do not support WAL's shared memory, so the ledger runs under TRUNCATE instead: one journal
//! beside the ledger, emptied at each commit, never deleted. It works on every disk the ledger
//! lands on. The ledger writes a few events per turn and does not need WAL's concurrency.
//!
//! [`SqliteEventStore::open`] would switch a file to WAL before migrating it, so the ledger
//! does not go through it. [`Store::open`] opens its own connection and, for a new file, copies
//! the schema happenstance builds in a template on a local temporary disk. The copy runs inside
//! one `BEGIN IMMEDIATE` transaction, so two processes creating the same ledger cannot both
//! write a schema.

use anyhow::{Context, Result, anyhow, bail};
use happenstance_core::{
    AppendCondition, AppendError, Event, MAX_TAG_LEN, Query, QueryItem, ReadOptions,
    SendEventStore, SequencePosition, Tag, Tags, collect,
};
use happenstance_sqlite::event_store::SqliteEventStore;
use happenstance_sqlite::rusqlite::{self, Connection};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use weighin_core::event::{EVENT_TYPES, EventEnvelope};
use weighin_core::{ItemId, Ledger};

/// The journal mode every ledger connection runs under, as `PRAGMA journal_mode` reports it.
pub const JOURNAL_MODE: &str = "truncate";

/// How long a connection waits for a lock, and for a WAL ledger to be released to TRUNCATE.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

pub struct Store {
    inner: SqliteEventStore,
    rt: tokio::runtime::Runtime,
}

/// What a conditioned append must not find.
#[derive(Debug, Clone)]
pub enum Guard {
    /// Fail if `item_raised` for this item exists.
    NotYetRaised(ItemId),
    /// Fail if any event for this item was appended after `position`.
    ItemUnchangedSince(ItemId, u64),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Appended {
    At(u64),
    /// The guard matched: someone else wrote first.
    Conflict,
}

fn tag(key: &str, value: &str) -> Option<Tag> {
    let mut v: String = value.chars().filter(|c| !c.is_control()).collect();
    let budget = MAX_TAG_LEN.saturating_sub(key.len() + 1);
    while v.len() > budget {
        v.pop();
    }
    if v.is_empty() {
        None
    } else {
        Tag::key_value(key, &v).ok()
    }
}

fn item_tags(id: &ItemId) -> Result<Tags> {
    Tags::from_pairs([("item", id.as_str())]).map_err(|e| anyhow!("{e}"))
}

impl Store {
    /// Open (creating when absent) the store at `path`.
    ///
    /// # Errors
    /// Fails when the file cannot be opened or migrated.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(d) = path.parent()
            && !d.as_os_str().is_empty()
        {
            std::fs::create_dir_all(d).with_context(|| format!("creating {}", d.display()))?;
        }
        let opened = (|| {
            set_aside_wal(path, true)?;
            let conn = connect(path)?;
            // TRUNCATE is in force, so no connection is using a WAL now; one left beside the
            // ledger is stale (its frames were checkpointed when the mode switched) and must not
            // be replayed by a later WAL opener.
            set_aside_wal(path, false)?;
            ensure_schema(&conn)?;
            SqliteEventStore::new(conn).map_err(|e| anyhow!("{e}"))
        })();
        let inner = opened.with_context(|| format!("opening ledger {}", path.display()))?;
        let rt = tokio::runtime::Builder::new_current_thread().build()?;
        Ok(Self { inner, rt })
    }

    /// Every ledger event, in position order.
    ///
    /// # Errors
    /// Fails on a store error or an undecodable payload.
    pub fn read_all(&self) -> Result<Vec<(u64, EventEnvelope)>> {
        let query = Query::from_item(
            QueryItem::of_types(EVENT_TYPES.iter().copied()).map_err(|e| anyhow!("{e}"))?,
        );
        let events = self
            .rt
            .block_on(collect(self.inner.read(&query, ReadOptions::new())))
            .map_err(|e| anyhow!("reading ledger: {e}"))?;
        let mut out = Vec::with_capacity(events.len());
        for se in events {
            let pos = u64::from(se.position);
            let env = EventEnvelope::from_json(se.event.data())
                .with_context(|| format!("decoding event at position {pos}"))?;
            out.push((pos, env));
        }
        Ok(out)
    }

    /// # Errors
    /// See [`Store::read_all`].
    pub fn ledger(&self) -> Result<Ledger> {
        let events = self.read_all()?;
        Ok(Ledger::fold(events.iter().map(|(p, e)| (*p, e))))
    }

    /// Append events for one item atomically under `guard`.
    ///
    /// # Errors
    /// Fails on a store error (a guard match is [`Appended::Conflict`], not an error).
    pub fn append(&self, envs: &[EventEnvelope], guard: &Guard) -> Result<Appended> {
        let mut events = Vec::with_capacity(envs.len());
        for env in envs {
            let tags: Vec<Tag> = env.tags().iter().filter_map(|(k, v)| tag(k, v)).collect();
            let tags = Tags::from_pairs(tags.iter().filter_map(|t| t.key().zip(t.value())))
                .map_err(|e| anyhow!("{e}"))?;
            let ev = Event::new(env.event.event_type(), env.to_json()?)
                .map_err(|e| anyhow!("{e}"))?
                .with_tags(tags);
            events.push(ev);
        }
        let condition = match guard {
            Guard::NotYetRaised(id) => Some(AppendCondition::new(Query::from_item(
                QueryItem::new(["item_raised"], item_tags(id)?).map_err(|e| anyhow!("{e}"))?,
            ))),
            Guard::ItemUnchangedSince(id, pos) => {
                let q = Query::from_item(
                    QueryItem::tagged(item_tags(id)?).map_err(|e| anyhow!("{e}"))?,
                );
                Some(AppendCondition::new(q).after_opt(SequencePosition::new(*pos)))
            }
        };
        match self
            .rt
            .block_on(self.inner.append(&events, condition.as_ref()))
        {
            Ok(p) => Ok(Appended::At(u64::from(p))),
            Err(AppendError::ConditionViolated(_)) => Ok(Appended::Conflict),
            Err(e) => Err(anyhow!("appending to ledger: {e}")),
        }
    }
}

/// `<ledger><suffix>`: the journal, WAL and shared-memory files SQLite keeps beside a database.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Rename a `-wal` beside the ledger aside; renaming is allowed where deleting is not.
///
/// * Before connecting (`only_if_empty`): SQLite deletes a `-wal` it finds beside an empty
///   database, and a failed WAL creation (weighin 0.1.x in a Cowork folder) leaves exactly that.
/// * After switching to TRUNCATE: converting a 0.1.x WAL ledger where deletes are refused
///   leaves its checkpointed `-wal` behind.
fn set_aside_wal(path: &Path, only_if_empty: bool) -> Result<()> {
    let wal = sibling(path, "-wal");
    let empty = std::fs::metadata(path).map_or(true, |m| m.len() == 0);
    if (empty || !only_if_empty) && wal.exists() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let aside = sibling(path, &format!("-wal.orphaned-{stamp}"));
        std::fs::rename(&wal, &aside)
            .with_context(|| format!("setting aside {}", wal.display()))?;
    }
    Ok(())
}

/// Open (creating when absent) a connection under [`JOURNAL_MODE`].
fn connect(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
    set_journal_mode(&conn)?;
    // FULL, not NORMAL: under a rollback journal NORMAL can lose the last commit on power loss.
    conn.execute_batch("PRAGMA synchronous = FULL; PRAGMA foreign_keys = ON;")?;
    Ok(conn)
}

/// Switch to [`JOURNAL_MODE`]. A ledger an older build left in WAL converts on first open; the
/// conversion needs every other connection gone, and SQLite reports that as busy without calling
/// the busy handler, so it is retried until [`BUSY_TIMEOUT`].
fn set_journal_mode(conn: &Connection) -> Result<()> {
    let deadline = Instant::now() + BUSY_TIMEOUT;
    let sql = format!("PRAGMA journal_mode = {JOURNAL_MODE}");
    loop {
        let result = conn.query_row(&sql, [], |row| row.get::<_, String>(0));
        let busy = match &result {
            Ok(mode) => !mode.eq_ignore_ascii_case(JOURNAL_MODE),
            Err(rusqlite::Error::SqliteFailure(f, _)) => matches!(
                f.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            ),
            Err(_) => false,
        };
        match result {
            Ok(mode) if mode.eq_ignore_ascii_case(JOURNAL_MODE) => return Ok(()),
            _ if busy && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(mode) => {
                bail!("SQLite kept journal mode {mode}; another process may hold the ledger open")
            }
            Err(e) => return Err(e.into()),
        }
    }
}

fn has_schema(conn: &Connection) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT count(*) FROM main.sqlite_master WHERE type = 'table' AND name = 'store_meta'",
        [],
        |row| row.get(0),
    )?;
    Ok(n > 0)
}

/// Give a new ledger happenstance's schema and a fresh store identity without letting
/// happenstance open it (see the module documentation).
fn ensure_schema(conn: &Connection) -> Result<()> {
    if has_schema(conn)? {
        return Ok(());
    }
    let tmp = tempfile::tempdir().context("creating a temporary directory")?;
    let template = tmp.path().join("template.sqlite3");
    drop(SqliteEventStore::open(&template).map_err(|e| anyhow!("building the template: {e}"))?);
    conn.execute(
        "ATTACH DATABASE ?1 AS tpl",
        [template.to_string_lossy().as_ref()],
    )?;
    let copied = copy_schema(conn);
    let _ = conn.execute_batch("DETACH DATABASE tpl");
    copied
}

fn copy_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let copied = (|| {
        // Another process may have created the schema while the template was built.
        if has_schema(conn)? {
            return Ok(());
        }
        let mut stmt = conn.prepare(
            "SELECT type, name, sql FROM tpl.sqlite_master
             WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'
             ORDER BY CASE type WHEN 'table' THEN 0 ELSE 1 END, rowid",
        )?;
        let objects = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        // Unqualified DDL creates its object in `main`.
        for (_, _, sql) in &objects {
            conn.execute_batch(sql)?;
        }
        for (_, name, _) in objects.iter().filter(|(ty, _, _)| ty == "table") {
            let name = name.replace('"', "\"\"");
            conn.execute(
                &format!("INSERT INTO main.\"{name}\" SELECT * FROM tpl.\"{name}\""),
                [],
            )?;
        }
        Ok(())
    })();
    match copied {
        Ok(()) => conn.execute_batch("COMMIT")?,
        Err(_) => {
            let _ = conn.execute_batch("ROLLBACK");
        }
    }
    copied
}

#[cfg(test)]
mod tests {
    use super::*;
    use weighin_core::event::{Channel, DismissReason, Dismissed, LedgerEvent, Raised};
    use weighin_core::marker::{Authority, Door, Kind};

    fn env(event: LedgerEvent) -> EventEnvelope {
        EventEnvelope {
            event,
            actor: "t".into(),
            session_id: Some("s1".into()),
            worktree: Some("wt".into()),
            branch: None,
            head: None,
            ts: "2026-09-26T00:00:00Z".into(),
        }
    }

    fn raise(id: &ItemId) -> EventEnvelope {
        env(LedgerEvent::ItemRaised(Raised {
            item_id: id.clone(),
            kind: Kind::Question,
            question: "q?".into(),
            did: None,
            flip: None,
            at: None,
            sym: None,
            blocks: None,
            door_hint: Door::OneWay,
            authority_hint: Authority::Authority,
            channel: Channel::Marker,
            quote: "q?".into(),
            source_path: None,
            source_line: None,
        }))
    }

    #[test]
    fn guards_make_writes_idempotent_and_detect_races() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("weighin/ledger.sqlite3")).unwrap();
        let id = ItemId::mint("s1", "q?");
        assert!(matches!(
            store
                .append(&[raise(&id)], &Guard::NotYetRaised(id.clone()))
                .unwrap(),
            Appended::At(1)
        ));
        assert_eq!(
            store
                .append(&[raise(&id)], &Guard::NotYetRaised(id.clone()))
                .unwrap(),
            Appended::Conflict,
            "second raise is a no-op"
        );

        let dismiss = env(LedgerEvent::ItemDismissed(Dismissed {
            item_id: id.clone(),
            why: DismissReason::Moot,
            note: String::new(),
            reason: "r".into(),
        }));
        // A writer that read at position 1 wins; a second writer that also read at 1 loses.
        assert!(matches!(
            store
                .append(
                    std::slice::from_ref(&dismiss),
                    &Guard::ItemUnchangedSince(id.clone(), 1)
                )
                .unwrap(),
            Appended::At(2)
        ));
        assert_eq!(
            store
                .append(&[dismiss], &Guard::ItemUnchangedSince(id.clone(), 1))
                .unwrap(),
            Appended::Conflict
        );

        let ledger = store.ledger().unwrap();
        assert_eq!(
            ledger.get(&id).unwrap().status,
            weighin_core::Status::Dismissed
        );
        assert_eq!(ledger.head, 2);
    }

    fn assert_truncate_layout(path: &Path) {
        let journal = sibling(path, "-journal");
        assert!(
            journal.exists(),
            "TRUNCATE keeps the journal instead of deleting it"
        );
        assert_eq!(std::fs::metadata(&journal).unwrap().len(), 0);
        assert!(!sibling(path, "-wal").exists(), "no WAL");
        assert!(!sibling(path, "-shm").exists(), "no shared memory");
    }

    #[test]
    fn ledger_runs_under_truncate_and_never_leaves_wal_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".weighin/ledger.sqlite3");
        let id = ItemId::mint("s1", "q?");
        {
            let store = Store::open(&path).unwrap();
            store
                .append(&[raise(&id)], &Guard::NotYetRaised(id.clone()))
                .unwrap();
            let mode: String = store
                .inner
                .settings()
                .map(|s| format!("{s:?}"))
                .unwrap_or_default();
            assert!(mode.contains("truncate"), "{mode}");
        }
        assert_truncate_layout(&path);
        // Reopening keeps the events and the identity.
        let store = Store::open(&path).unwrap();
        assert_eq!(store.ledger().unwrap().head, 1);
        drop(store);
        assert_truncate_layout(&path);
    }

    #[test]
    fn empty_leftovers_from_a_failed_creation_are_recovered() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.sqlite3");
        std::fs::write(&path, b"").unwrap();
        std::fs::write(sibling(&path, "-wal"), b"not a wal").unwrap();
        let store = Store::open(&path).unwrap();
        let id = ItemId::mint("s1", "q?");
        assert!(matches!(
            store
                .append(&[raise(&id)], &Guard::NotYetRaised(id.clone()))
                .unwrap(),
            Appended::At(1)
        ));
        drop(store);
        assert_truncate_layout(&path);
        let orphans = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains("-wal.orphaned-"))
            .count();
        assert_eq!(orphans, 1, "the orphan WAL is renamed aside, not deleted");
    }

    #[test]
    fn a_stale_wal_beside_a_truncate_ledger_is_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.sqlite3");
        let id = ItemId::mint("s1", "q?");
        {
            let store = Store::open(&path).unwrap();
            store
                .append(&[raise(&id)], &Guard::NotYetRaised(id.clone()))
                .unwrap();
        }
        std::fs::write(sibling(&path, "-wal"), b"stale frames").unwrap();
        let store = Store::open(&path).unwrap();
        assert_eq!(store.ledger().unwrap().head, 1);
        drop(store);
        assert_truncate_layout(&path);
    }

    #[test]
    fn a_wal_ledger_from_an_older_build_converts_and_keeps_its_events() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.sqlite3");
        let id = ItemId::mint("s1", "q?");
        {
            // What weighin 0.1.x did: happenstance's own open, which leaves the file in WAL.
            let old = SqliteEventStore::open(&path).unwrap();
            let rt = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            let store = Store { inner: old, rt };
            store
                .append(&[raise(&id)], &Guard::NotYetRaised(id.clone()))
                .unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.ledger().unwrap().head, 1);
        assert_eq!(
            store
                .append(&[raise(&id)], &Guard::NotYetRaised(id.clone()))
                .unwrap(),
            Appended::Conflict
        );
        drop(store);
        assert_truncate_layout(&path);
    }

    #[test]
    fn concurrent_creators_share_one_schema_and_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.sqlite3");
        let handles: Vec<_> = (0..6)
            .map(|n| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let store = Store::open(&path).unwrap();
                    let id = ItemId::mint(&format!("s{n}"), "q?");
                    store
                        .append(&[raise(&id)], &Guard::NotYetRaised(id.clone()))
                        .unwrap()
                })
            })
            .collect();
        for h in handles {
            assert!(matches!(h.join().unwrap(), Appended::At(_)));
        }
        assert_eq!(Store::open(&path).unwrap().ledger().unwrap().head, 6);
    }
}

//! Ledger operations. Every command, every hook and the `batch` entry point goes through
//! [`Session`], which is the single write path: it reads the ledger, validates, appends under a
//! guard, and performs the side effects (atoms, source annotations) that follow a decision.

use anyhow::{Context as _, Result, anyhow, bail};
use serde::Deserialize;
use std::fmt::Write as _;
use std::path::Path;
use weighin_core::atom::{self, RenderContext};
use weighin_core::card::Card;
use weighin_core::event::{
    Acknowledged, Blast, Channel, Decided, DefaultCause, Defaulted, Deferred, DismissReason,
    Dismissed, Grounded, LedgerEvent, Linked, Presented, Provenance, Raised, RecordWritten,
    Reopened, Route, Superseded, Timing, Triaged,
};
use weighin_core::id::{EXTENDED_HEX_LEN, ItemId};
use weighin_core::marker::{Authority, Door, Kind, Marker, authority_tripwire};
use weighin_core::{Item, Ledger, Status};

use crate::annotate::{Skip, Syntax, annotate_line};
use crate::env::{self, Env};
use crate::git;
use crate::store::{Appended, Guard, Store};

/// What an agent or a person raises.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub(crate) struct RaiseInput {
    pub(crate) q: String,
    pub(crate) kind: Option<String>,
    pub(crate) door: Option<String>,
    pub(crate) authority: Option<String>,
    pub(crate) did: Option<String>,
    pub(crate) flip: Option<String>,
    pub(crate) at: Option<String>,
    pub(crate) sym: Option<String>,
    pub(crate) blocks: Option<String>,
    pub(crate) channel: Option<String>,
    pub(crate) quote: Option<String>,
    pub(crate) source_path: Option<String>,
    pub(crate) source_line: Option<usize>,
}

impl RaiseInput {
    #[must_use]
    pub(crate) fn from_marker(m: &Marker, channel: Channel) -> Self {
        Self {
            q: m.question.clone(),
            kind: Some(m.kind.as_str().into()),
            door: Some(m.door.as_str().into()),
            authority: Some(m.authority.as_str().into()),
            did: m.did.clone(),
            flip: m.flip.clone(),
            at: m.at.clone(),
            sym: m.sym.clone(),
            blocks: m.blocks.clone(),
            channel: Some(channel.as_str().into()),
            quote: Some(m.raw.clone()),
            source_path: None,
            source_line: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub(crate) enum Op {
    Raise(RaiseInput),
    Triage(TriageOp),
    Ground {
        id: String,
        card: Card,
        #[serde(default)]
        reason: Option<String>,
    },
    Present {
        id: String,
        #[serde(default)]
        surface: Option<String>,
    },
    Decide(DecideOp),
    Defer(DeferOp),
    Default {
        id: String,
        option: String,
        cause: String,
        reason: String,
    },
    Dismiss {
        id: String,
        why: String,
        #[serde(default)]
        note: Option<String>,
        reason: String,
    },
    Reopen {
        id: String,
        #[serde(default)]
        note: Option<String>,
        #[serde(default)]
        reason: String,
    },
    Supersede {
        id: String,
        by: String,
        reason: String,
    },
    Link {
        id: String,
        kind: String,
        target: String,
        #[serde(default)]
        reason: Option<String>,
    },
    Ack {
        id: String,
        #[serde(default)]
        reason: Option<String>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RaiseOutcome {
    New(ItemId),
    /// The same session raised it before (a hook firing twice).
    Existing(ItemId),
    /// Another open item already asks this; linked to it instead.
    Duplicate(ItemId),
}

impl RaiseOutcome {
    #[must_use]
    pub(crate) fn id(&self) -> &ItemId {
        match self {
            Self::New(i) | Self::Existing(i) | Self::Duplicate(i) => i,
        }
    }
}

/// The ledger as it stands, for commands that only read, or `None` when no store exists yet.
///
/// Never creates the store. A ledger is created by the first item raised, and a folder that
/// refuses deletes (B-01) could not be rid of one a read left behind.
///
/// # Errors
/// Fails when the store's path cannot be checked (so an unreadable ledger is never taken for a
/// missing one), or when an existing store cannot be opened or read.
pub(crate) fn stored_ledger(env: &Env) -> Result<Option<Ledger>> {
    existing_store(env)?.map(|s| s.ledger()).transpose()
}

/// The store, opened only when it exists already. See [`stored_ledger`].
fn existing_store(env: &Env) -> Result<Option<Store>> {
    let path = env.store_path();
    if path
        .try_exists()
        .with_context(|| format!("checking {}", path.display()))?
    {
        Store::open(&path).map(Some)
    } else {
        Ok(None)
    }
}

/// [`stored_ledger`], with no store read as an empty ledger.
///
/// # Errors
/// See [`stored_ledger`].
pub(crate) fn read_ledger(env: &Env) -> Result<Ledger> {
    Ok(stored_ledger(env)?.unwrap_or_default())
}

/// The working tree's hash, read from git at most once per session.
#[derive(Debug)]
enum TreeCache {
    Unread,
    /// `None` when there is no repository or git could not hash the tree.
    Read(Option<String>),
}

/// `Op::Triage`: route an item, naming its door and blast radius.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TriageOp {
    pub(crate) id: String,
    pub(crate) door: String,
    pub(crate) blast: String,
    #[serde(default)]
    pub(crate) authority: Option<String>,
    pub(crate) route: String,
    #[serde(default)]
    pub(crate) timing: Option<Timing>,
    #[serde(default)]
    pub(crate) resolve_by: Option<String>,
    pub(crate) reason: String,
}

/// `Op::Decide`: record the option chosen. `note` defaults to empty and `via` to `chat`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DecideOp {
    pub(crate) id: String,
    pub(crate) option: String,
    #[serde(default)]
    pub(crate) note: Option<String>,
    #[serde(default)]
    pub(crate) provenance: Option<String>,
    #[serde(default)]
    pub(crate) via: Option<String>,
    #[serde(default)]
    pub(crate) seconds: Option<u64>,
}

/// `Op::Defer`: wait for a named fact, with a trigger, a resolve-by and a default.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DeferOp {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) flip: String,
    pub(crate) trigger: String,
    pub(crate) resolve_by: String,
    pub(crate) default: String,
    #[serde(default)]
    pub(crate) owner: Option<String>,
    #[serde(default)]
    pub(crate) unqualified: bool,
    #[serde(default)]
    pub(crate) reason: String,
}

/// The one event an operation appends to an item, and the receipt printed for it.
struct Change {
    event: LedgerEvent,
    receipt: String,
}

/// How a raise was routed, read from its hints (unknown or missing values take the defaults).
#[derive(Clone, Copy)]
struct Hints {
    channel: Channel,
    door: Door,
    authority: Authority,
}

impl Hints {
    fn of(input: &RaiseInput) -> Self {
        let channel = match input.channel.as_deref() {
            Some("scan") => Channel::Scan,
            Some("sweep") => Channel::Sweep,
            Some("manual") => Channel::Manual,
            _ => Channel::Marker,
        };
        Self {
            channel,
            door: input.door.as_deref().map(Door::parse).unwrap_or_default(),
            authority: input
                .authority
                .as_deref()
                .map(Authority::parse)
                .unwrap_or_default(),
        }
    }
}

pub(crate) struct Session<'a> {
    pub(crate) env: &'a Env,
    /// The store, once one exists: it is created by the first append, so a command that fails
    /// before writing anything leaves no ledger behind (B-01).
    store: Option<Store>,
    pub(crate) ledger: Ledger,
    tree: TreeCache,
    actor: String,
    /// One slot per raise op earlier in this batch, so later ops can say `@1`, `@2`, ...: the
    /// raised id, or `None` when that raise failed (it keeps its place, so no ref shifts).
    raised: Vec<Option<ItemId>>,
    /// The date atoms and source annotations carry (`YYYY-MM-DD`), read once when the session
    /// opens so every write in a batch carries the same day.
    today: String,
}

impl<'a> Session<'a> {
    /// Open the session on the store, if there is one yet; it is created by the first append.
    ///
    /// # Errors
    /// Fails when the store's path cannot be checked, or an existing store cannot be opened or
    /// read.
    pub(crate) fn open(env: &'a Env, actor: &str) -> Result<Self> {
        let store = existing_store(env)?;
        let ledger = match &store {
            Some(s) => s.ledger()?,
            None => Ledger::default(),
        };
        Ok(Self {
            env,
            store,
            ledger,
            tree: TreeCache::Unread,
            actor: actor.to_owned(),
            raised: Vec::new(),
            today: env::today(),
        })
    }

    /// Fix the date this session writes, so a test does not depend on the wall clock.
    #[cfg(test)]
    fn with_today(mut self, today: &str) -> Self {
        today.clone_into(&mut self.today);
        self
    }

    fn tree_hash(&mut self) -> Option<String> {
        if let TreeCache::Read(t) = &self.tree {
            // The cache keeps its copy for the next event; the caller's event owns this one.
            return t.clone();
        }
        let t = self.env.repo_root.as_ref().and_then(|r| git::tree_hash(r));
        self.tree = TreeCache::Read(t.clone());
        t
    }

    /// The store, created on the first call when there was none.
    fn store(&mut self) -> Result<&Store> {
        let env = self.env;
        match &mut self.store {
            Some(store) => Ok(store),
            slot @ None => Ok(slot.insert(Store::open(&env.store_path())?)),
        }
    }

    /// Append events for `id` guarded on the item being unchanged since this session read it.
    fn write(&mut self, id: &ItemId, events: Vec<LedgerEvent>) -> Result<()> {
        let since = self.ledger.get(id).map_or(0, |i| i.last_position);
        let envs: Vec<_> = events
            .into_iter()
            .map(|e| self.env.envelope(e, &self.actor))
            .collect();
        match self
            .store()?
            .append(&envs, &Guard::ItemUnchangedSince(id.clone(), since))?
        {
            Appended::At(last) => {
                let n = u64::try_from(envs.len()).context("too many events in one append")?;
                let first = last + 1 - n;
                for (position, e) in (first..).zip(&envs) {
                    self.ledger.apply(position, e);
                }
                Ok(())
            }
            Appended::Conflict => {
                self.ledger = self.store()?.ledger()?;
                bail!(
                    "{id} changed while this was being written (another session acted on it); re-read with `weighin show {id}` and retry"
                )
            }
        }
    }

    fn item(&self, needle: &str) -> Result<Item> {
        if let Some(n) = needle.strip_prefix('@') {
            // The parse error adds nothing the message does not already say.
            let Ok(k) = n.parse::<usize>() else {
                bail!("`{needle}`: refs are @1, @2, ... (raises earlier in this batch)");
            };
            let slot = self.raised.get(k.wrapping_sub(1)).ok_or_else(|| {
                anyhow!(
                    "`{needle}`: only {} raise(s) so far in this batch",
                    self.raised.len()
                )
            })?;
            let Some(id) = slot else {
                bail!("`{needle}`: raise {k} in this batch failed, so it names no item");
            };
            return self
                .ledger
                .get(id)
                .cloned()
                .ok_or_else(|| anyhow!("{id} vanished"));
        }
        self.ledger
            .resolve(needle)
            .cloned()
            .ok_or_else(|| anyhow!("no single item matches `{needle}` (try `weighin open --all`)"))
    }

    /// Raise a question. Idempotent per session; an open duplicate from another session is linked.
    ///
    /// # Errors
    /// Fails on an empty question or a store error.
    pub(crate) fn raise(&mut self, input: &RaiseInput) -> Result<RaiseOutcome> {
        let outcome = self.raise_item(input)?;
        if let RaiseOutcome::New(id) = &outcome {
            self.default_acted_call(input, id)?;
        }
        Ok(outcome)
    }

    /// [`Session::raise`] up to the item's existing: its acted-call default is the caller's.
    fn raise_item(&mut self, input: &RaiseInput) -> Result<RaiseOutcome> {
        let q = input.q.trim();
        if q.is_empty() {
            bail!("a raised item needs a question");
        }
        let session = self
            .env
            .session
            .clone()
            .unwrap_or_else(|| "no-session".into());
        let minted = ItemId::mint(&session, q);
        if let Some(known) = self.already_raised(q, &session, &minted)? {
            return Ok(known);
        }
        self.append_raised(input, minted, &session)
    }

    /// The item already asking `q`, if any, noting that `session` raised it again.
    fn already_raised(
        &mut self,
        q: &str,
        session: &str,
        minted: &ItemId,
    ) -> Result<Option<RaiseOutcome>> {
        if let Some(closed) = self
            .ledger
            .any_with_question(q)
            .filter(|i| !i.status.is_open())
        {
            // Settled (decided, defaulted, dismissed): never re-asked. Record the re-raise only.
            let cid = closed.id.clone();
            if closed.session_id.as_deref() != Some(session)
                && !closed.also_raised_in.iter().any(|s| s.as_str() == session)
            {
                self.write(
                    &cid,
                    vec![LedgerEvent::ItemLinked(Linked {
                        item_id: cid.clone(),
                        kind: "raised-again".into(),
                        target: session.to_owned(),
                        reason: "a settled question was raised again; not re-asked".into(),
                    })],
                )?;
            }
            return Ok(Some(RaiseOutcome::Existing(cid)));
        }
        let Some(existing) = self.ledger.open_with_question(q) else {
            return Ok(None);
        };
        let eid = existing.id.clone();
        if &eid == minted || existing.session_id.as_deref() == Some(session) {
            return Ok(Some(RaiseOutcome::Existing(eid)));
        }
        if !existing
            .also_raised_in
            .iter()
            .any(|s| s.as_str() == session)
        {
            self.write(
                &eid,
                vec![LedgerEvent::ItemLinked(Linked {
                    item_id: eid.clone(),
                    kind: "raised-again".into(),
                    target: session.to_owned(),
                    reason: "the same question was raised in another session".into(),
                })],
            )?;
        }
        Ok(Some(RaiseOutcome::Duplicate(eid)))
    }

    /// Append the raise under `minted`, or under its extended form when another question holds
    /// the short id. `Existing` when a racing writer raised the same question first.
    fn append_raised(
        &mut self,
        input: &RaiseInput,
        minted: ItemId,
        session: &str,
    ) -> Result<RaiseOutcome> {
        let q = input.q.trim();
        let mut id = minted;
        for _ in 0..2 {
            let env = self
                .env
                .envelope(self.raised_event(input, id.clone()), &self.actor);
            match self
                .store()?
                .append(std::slice::from_ref(&env), &Guard::NotYetRaised(id.clone()))?
            {
                Appended::At(p) => {
                    self.ledger.apply(p, &env);
                    return Ok(RaiseOutcome::New(id));
                }
                Appended::Conflict => {
                    self.ledger = self.store()?.ledger()?;
                    let same = self.ledger.get(&id).is_some_and(|i| {
                        weighin_core::id::question_key(&i.question)
                            == weighin_core::id::question_key(q)
                    });
                    if same {
                        return Ok(RaiseOutcome::Existing(id));
                    }
                    id = ItemId::mint_with_len(session, q, EXTENDED_HEX_LEN);
                }
            }
        }
        bail!("could not mint a free id for this question")
    }

    fn raised_event(&self, input: &RaiseInput, id: ItemId) -> LedgerEvent {
        let q = input.q.trim();
        let hints = Hints::of(input);
        LedgerEvent::ItemRaised(Raised {
            item_id: id,
            kind: input.kind.as_deref().map(Kind::parse).unwrap_or_default(),
            question: q.to_owned(),
            did: input.did.clone(),
            flip: input.flip.clone(),
            at: input.at.as_deref().map(|a| self.env.relative_anchor(a)),
            sym: input.sym.clone(),
            blocks: input.blocks.clone(),
            door_hint: hints.door,
            authority_hint: hints.authority,
            channel: hints.channel,
            quote: input.quote.clone().unwrap_or_else(|| q.to_owned()),
            source_path: input.source_path.clone(),
            source_line: input.source_line,
        })
    }

    /// Decision wi-4d8b6a: a judgment-bound two-way door the agent already acted on is its own
    /// call, logged as a default and listed in the digest; it never becomes a question.
    fn default_acted_call(&mut self, input: &RaiseInput, id: &ItemId) -> Result<()> {
        let hints = Hints::of(input);
        let did = input
            .did
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty() && !d.eq_ignore_ascii_case("blocked"));
        // The tripwire keeps authority-sounding questions (compliance, isolation, retention, ...)
        // open for the sitting even when the agent called them judgment calls.
        let tripwire = authority_tripwire(&format!(
            "{} {} {}",
            input.q.trim(),
            did.unwrap_or(""),
            input.flip.as_deref().unwrap_or("")
        ));
        if hints.channel == Channel::Marker
            && hints.door == Door::TwoWay
            && hints.authority == Authority::Judgment
            && tripwire.is_none()
            && let Some(did) = did
        {
            self.write(id, vec![LedgerEvent::ItemDefaulted(Defaulted { item_id: id.clone(), option: did.to_owned(), cause: DefaultCause::TwoWay, reason: "judgment-bound two-way door the agent acted on; logged, reversible from the digest".into() })])?;
        }
        Ok(())
    }

    /// Apply one operation and return a one-line receipt.
    ///
    /// # Errors
    /// Fails when the operation is invalid for the item or the store refuses it.
    pub(crate) fn apply(&mut self, op: &Op) -> Result<String> {
        match op {
            Op::Raise(r) => self.apply_raise(r),
            Op::Present { id, surface } => self.present(id, surface.as_deref()),
            Op::Decide(d) => self.decide(d),
            Op::Defer(d) => self.defer(d),
            Op::Triage(t) => self.change(&t.id, |item| triage_event(item, t)),
            Op::Ground { id, card, reason } => {
                self.change(id, |item| ground_event(item, card, reason.as_deref()))
            }
            Op::Default {
                id,
                option,
                cause,
                reason,
            } => self.change(id, |item| default_event(item, option, cause, reason)),
            Op::Dismiss {
                id,
                why,
                note,
                reason,
            } => self.change(id, |item| dismiss_event(item, why, note.as_deref(), reason)),
            Op::Reopen { id, note, reason } => {
                self.change(id, |item| reopen_event(item, note.as_deref(), reason))
            }
            Op::Supersede { id, by, reason } => {
                self.change(id, |item| Ok(supersede_event(item, by, reason)))
            }
            Op::Link {
                id,
                kind,
                target,
                reason,
            } => self.change(id, |item| {
                Ok(link_event(item, kind, target, reason.as_deref()))
            }),
            Op::Ack { id, reason } => self.change(id, |item| ack_event(item, reason.as_deref())),
        }
    }

    /// Raise `r` into the next `@N` slot. The slot is taken even when the raise fails, and is
    /// filled as soon as the item exists, before its acted-call default is written.
    fn apply_raise(&mut self, r: &RaiseInput) -> Result<String> {
        let raised = self.raise_item(r);
        // The slot keeps its own copy of the id; the outcome goes on to the receipt.
        self.raised
            .push(raised.as_ref().ok().map(|o| o.id().clone()));
        let out = raised?;
        if let RaiseOutcome::New(id) = &out {
            self.default_acted_call(r, id)?;
        }
        let status = self.ledger.get(out.id()).map_or("?", |i| i.status.as_str());
        Ok(match out {
            RaiseOutcome::New(id) => format!("raised {id} ({status})"),
            RaiseOutcome::Existing(id) => format!("{id} already raised"),
            RaiseOutcome::Duplicate(id) => format!("linked to open {id}"),
        })
    }

    /// Note a raise op in this batch that could not be parsed: it still takes its `@N` slot, so
    /// a ref to it is refused and later refs keep their place.
    pub(crate) fn skip_raise(&mut self) {
        self.raised.push(None);
    }

    /// Resolve `id`, let `build` validate it and name its one event, append that, and return the
    /// receipt.
    fn change(&mut self, id: &str, build: impl FnOnce(&Item) -> Result<Change>) -> Result<String> {
        let item = self.item(id)?;
        let Change { event, receipt } = build(&item)?;
        self.write(&item.id, vec![event])?;
        Ok(receipt)
    }

    fn present(&mut self, id: &str, surface: Option<&str>) -> Result<String> {
        let item = self.item(id)?;
        if !item.status.is_open() {
            bail!("{} is {}, not open", item.id, item.status.as_str());
        }
        let tree = self.tree_hash();
        let surface = surface.unwrap_or("chat");
        self.write(
            &item.id,
            vec![LedgerEvent::ItemPresented(Presented {
                item_id: item.id.clone(),
                surface: surface.to_owned(),
                tree_hash: tree,
                reason: format!("shown in {surface}"),
            })],
        )?;
        Ok(format!("presented {} ({surface})", item.id))
    }

    fn defer(&mut self, d: &DeferOp) -> Result<String> {
        let item = self.item(&d.id)?;
        check_deferral(&item, d)?;
        let owner = d
            .owner
            .clone()
            .unwrap_or_else(|| self.env.config.owner.clone());
        self.write(
            &item.id,
            vec![LedgerEvent::ItemDeferred(Deferred {
                item_id: item.id.clone(),
                flip: d.flip.clone(),
                trigger: d.trigger.clone(),
                resolve_by: d.resolve_by.clone(),
                default_option: d.default.clone(),
                owner,
                unqualified: d.unqualified,
                reason: d.reason.clone(),
            })],
        )?;
        let mut receipt = format!(
            "deferred {} until {} (default {} by {})",
            item.id, d.trigger, d.default, d.resolve_by
        );
        if item.door() == Door::OneWay
            && let Some(path) = self.write_open_question(&item.id)?
        {
            write!(receipt, " · tracking atom {path}")?;
        }
        Ok(receipt)
    }

    fn decide(&mut self, d: &DecideOp) -> Result<String> {
        let item = self.item(&d.id)?;
        let matched = check_answer(&item, &d.option)?;
        let provenance = match d.provenance.as_deref() {
            Some("user-approved") => Provenance::UserApproved,
            Some("user-directed") => Provenance::UserDirected,
            _ if matched == Some(true) => Provenance::UserApproved,
            _ => Provenance::UserDirected,
        };
        let now = env::now();
        let seconds = d.seconds.or_else(|| {
            item.presented_ts
                .as_deref()
                .and_then(|p| env::seconds_between(p, &now))
        });
        let tree = self.tree_hash();
        let decider = self.env.human();
        self.write(
            &item.id,
            vec![LedgerEvent::ItemDecided(Decided {
                item_id: item.id.clone(),
                option: d.option.clone(),
                decider,
                provenance,
                note: d.note.clone().unwrap_or_default(),
                tree_hash: tree,
                matched_recommendation: matched,
                via: d.via.clone().unwrap_or_else(|| "chat".into()),
                seconds_to_decide: seconds,
                reason: match matched {
                    Some(true) => "chose the recommended option".into(),
                    Some(false) => "chose against the recommendation".into(),
                    None => "answered".into(),
                },
            })],
        )?;
        self.decision_receipt(&item.id, &d.option, matched)
    }

    /// Write the decision's atom and source annotation, and say what was written.
    fn decision_receipt(
        &mut self,
        id: &ItemId,
        option: &str,
        matched: Option<bool>,
    ) -> Result<String> {
        let mut receipt = format!("decided {id} → {option}");
        if matched == Some(false) {
            receipt.push_str(" (override)");
        }
        if let Some(path) = self.write_decision_atom(id)? {
            write!(receipt, " · atom {path}")?;
        }
        match self.annotate_source(id, option)? {
            Some(Annotation::Written(src)) => write!(receipt, " · annotated {src}")?,
            Some(Annotation::Skipped(src, skip)) => {
                write!(receipt, " · not annotated {src} ({})", skip.reason())?;
            }
            None => {}
        }
        write!(receipt, " · trailer `Weigh-In: {id} decided {option}`")?;
        Ok(receipt)
    }

    fn write_file(&self, rel: &str, text: &str) -> Result<()> {
        let path = self.env.root().join(rel);
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(&path, text.replace("\r\n", "\n"))?;
        Ok(())
    }

    fn write_decision_atom(&mut self, id: &ItemId) -> Result<Option<String>> {
        let Some(item) = self.ledger.get(id).cloned() else {
            return Ok(None);
        };
        let Some(text) = atom::render_decision(&item, &render_ctx(&self.today)) else {
            return Ok(None);
        };
        let rel = atom::decision_path(&item);
        self.write_file(&rel, &text)?;
        self.write(
            id,
            vec![LedgerEvent::RecordWritten(RecordWritten {
                item_id: id.clone(),
                path: rel.clone(),
                atom_id: atom::decision_atom_id(&item),
                atom_kind: "decision".into(),
                reason: "decision rendered to .kb/_intake for kb-ingest".into(),
            })],
        )?;
        Ok(Some(rel))
    }

    fn write_open_question(&mut self, id: &ItemId) -> Result<Option<String>> {
        let Some(item) = self.ledger.get(id).cloned() else {
            return Ok(None);
        };
        let Some(text) = atom::render_open_question(&item, &render_ctx(&self.today)) else {
            return Ok(None);
        };
        let rel = atom::open_question_path(&item);
        self.write_file(&rel, &text)?;
        self.write(
            id,
            vec![LedgerEvent::RecordWritten(RecordWritten {
                item_id: id.clone(),
                path: rel.clone(),
                atom_id: atom::open_question_atom_id(&item),
                atom_kind: "open_question".into(),
                reason: "one-way item deferred past its session".into(),
            })],
        )?;
        Ok(Some(rel))
    }

    /// spec-kit's write-back: the line the question came from gets an arrow, so it is not raised
    /// again. The arrow goes only inside the line's comment ([`annotate_line`]); a line it cannot
    /// go in is left as it was, and the receipt says so.
    fn annotate_source(&self, id: &ItemId, option: &str) -> Result<Option<Annotation>> {
        let Some(item) = self.ledger.get(id) else {
            return Ok(None);
        };
        let Some(src) = &item.source_path else {
            return Ok(None);
        };
        let path = self.env.root().join(src);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Ok(None);
        };
        let first = item.quote.lines().next().unwrap_or("").trim();
        if first.is_empty() {
            return Ok(None);
        }
        let syntax = if is_markdown(src) {
            Syntax::Markdown
        } else {
            Syntax::Code
        };
        let note = format!("→ Weigh-In {id}: decided {option} ({})", self.today);
        // Rebuild with each line's own terminator so a CRLF file stays CRLF. `found` is the
        // outcome on the question's line: `None` until it is found, then the skip, if any.
        let mut found = None;
        let mut joined = String::with_capacity(text.len() + 64);
        for chunk in text.split_inclusive('\n') {
            let (body, end) = match chunk.strip_suffix("\r\n") {
                Some(b) => (b, "\r\n"),
                None => match chunk.strip_suffix('\n') {
                    Some(b) => (b, "\n"),
                    None => (chunk, ""),
                },
            };
            if found.is_none() && body.trim() == first && !body.contains("→ Weigh-In") {
                let rewritten = annotate_line(body, &note, syntax);
                joined.push_str(rewritten.as_deref().unwrap_or(body));
                found = Some(rewritten.err());
            } else {
                joined.push_str(body);
            }
            joined.push_str(end);
        }
        let annotation = match found {
            None => return Ok(None),
            Some(Some(skip)) => Annotation::Skipped(src.clone(), skip),
            Some(None) => {
                std::fs::write(&path, joined)?;
                Annotation::Written(src.clone())
            }
        };
        Ok(Some(annotation))
    }
}

/// What a decision did to the line its question came from.
enum Annotation {
    /// The arrow was written into this file.
    Written(String),
    /// This file was left as it was, for this reason.
    Skipped(String, Skip),
}

/// Whether `rel` is markdown: scanned for question bullets rather than TODO comments, and
/// annotated as prose.
#[must_use]
pub(crate) fn is_markdown(rel: &str) -> bool {
    Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "md" | "mdx" | "markdown"))
}

fn render_ctx(today: &str) -> RenderContext {
    RenderContext {
        date: today.to_owned(),
        depends_on: Vec::new(),
        source_paths: Vec::new(),
    }
}

fn triage_event(item: &Item, t: &TriageOp) -> Result<Change> {
    let blast = Blast::parse(&t.blast)
        .ok_or_else(|| anyhow!("blast must be file, module, system or external"))?;
    let route = match t.route.as_str() {
        "present" => Route::Present,
        "default" => Route::Default,
        "dismiss" => Route::Dismiss,
        _ => bail!("route must be present, default or dismiss"),
    };
    let door = Door::parse(&t.door);
    let authority = t
        .authority
        .as_deref()
        .map_or_else(|| item.authority(), Authority::parse);
    if route == Route::Default && (door != Door::TwoWay || authority != Authority::Judgment) {
        bail!(
            "{}: only a judgment-bound two-way door may be defaulted (door {}, {})",
            item.id,
            door.as_str(),
            authority.as_str()
        );
    }
    Ok(Change {
        event: LedgerEvent::ItemTriaged(Triaged {
            item_id: item.id.clone(),
            door,
            blast,
            authority,
            route,
            timing: t.timing.clone(),
            resolve_by: t.resolve_by.clone(),
            reason: t.reason.clone(),
        }),
        receipt: format!(
            "triaged {} · {} · {} · {}",
            item.id,
            door.as_str(),
            blast.as_str(),
            route_str(route)
        ),
    })
}

fn ground_event(item: &Item, card: &Card, reason: Option<&str>) -> Result<Change> {
    card.validate().map_err(|e| anyhow!("{}: {e}", item.id))?;
    let earned = if card.recommendation_earned() {
        "recommendation earned"
    } else {
        "options first"
    };
    Ok(Change {
        event: LedgerEvent::ItemGrounded(Grounded {
            item_id: item.id.clone(),
            card: card.clone(),
            reason: reason.unwrap_or("grounded").to_owned(),
        }),
        receipt: format!(
            "grounded {} · {} options · {earned}",
            item.id,
            card.options.len()
        ),
    })
}

/// The option `option` of an open item, checked against its card: `Some(matched)` says whether
/// it is the recommendation, `None` that the item has no card.
fn check_answer(item: &Item, option: &str) -> Result<Option<bool>> {
    if !item.status.is_open() {
        bail!(
            "{} is {}, not open; reopen it first",
            item.id,
            item.status.as_str()
        );
    }
    let Some(card) = &item.card else {
        return Ok(None);
    };
    let opt = card.option(option).ok_or_else(|| {
        anyhow!(
            "{} has no option `{option}` (options: {})",
            item.id,
            card.options
                .iter()
                .map(|o| o.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    if let Some(m) = &opt.fails_must {
        bail!("{}: option {option} fails a MUST ({m})", item.id);
    }
    Ok(Some(card.recommendation.option == option))
}

fn check_deferral(item: &Item, d: &DeferOp) -> Result<()> {
    if !item.status.is_open() {
        bail!("{} is {}, not open", item.id, item.status.as_str());
    }
    if let Some(card) = &item.card
        && card.option(&d.default).is_none()
    {
        bail!("{}: default `{}` is not an option", item.id, d.default);
    }
    // A bare "defer" from the user is recorded as unqualified: it names no flip fact and is
    // counted against the action rate.
    for (k, v) in [
        ("flip", if d.unqualified { &d.trigger } else { &d.flip }),
        ("trigger", &d.trigger),
        ("resolve_by", &d.resolve_by),
    ] {
        if v.trim().is_empty() {
            bail!("a deferral needs {k}: waiting without a named fact is the can being kicked");
        }
    }
    Ok(())
}

fn default_event(item: &Item, option: &str, cause: &str, reason: &str) -> Result<Change> {
    let cause = DefaultCause::parse(cause)
        .ok_or_else(|| anyhow!("cause must be two-way, expired or unattended"))?;
    if item.door() != Door::TwoWay {
        bail!(
            "{} is a {} door; only a two-way door is defaulted (triage it first)",
            item.id,
            item.door().as_str()
        );
    }
    if cause != DefaultCause::Expired && item.authority() != Authority::Judgment {
        bail!(
            "{} is authority-bound; only its resolve-by expiring may default it (triage it as judgment if that is wrong)",
            item.id
        );
    }
    Ok(Change {
        event: LedgerEvent::ItemDefaulted(Defaulted {
            item_id: item.id.clone(),
            option: option.to_owned(),
            cause,
            reason: reason.to_owned(),
        }),
        receipt: format!("defaulted {} to {option} ({})", item.id, cause.as_str()),
    })
}

fn dismiss_event(item: &Item, why: &str, note: Option<&str>, reason: &str) -> Result<Change> {
    let why = DismissReason::parse(why).ok_or_else(|| anyhow!("why must be not-useful, recoverable, moot, already-decided:<ref>, duplicate-of:<id> or not-my-call:<who>"))?;
    // Papering-over guard: an authority-sounding item is "already decided" only by an accepted
    // .kb decision or by the user's own words choosing it.
    if let DismissReason::AlreadyDecided(r) = &why
        && let Some(word) = authority_tripwire(&format!("{} {}", item.question, item.quote))
        && !(r.starts_with("kb-") || r.starts_with("chat:"))
    {
        bail!(
            "{} is authority-bound (`{word}`): cite the accepted atom (already-decided:kb-…) or the user's words choosing it (already-decided:chat:\"…\"). \"Assume and keep going\" delegates the work, not this decision; present it instead",
            item.id
        );
    }
    let receipt = format!("dismissed {} ({})", item.id, why.label());
    Ok(Change {
        event: LedgerEvent::ItemDismissed(Dismissed {
            item_id: item.id.clone(),
            why,
            note: note.unwrap_or_default().to_owned(),
            reason: reason.to_owned(),
        }),
        receipt,
    })
}

fn reopen_event(item: &Item, note: Option<&str>, reason: &str) -> Result<Change> {
    if item.status.is_open() && item.status != Status::Deferred {
        bail!("{} is already open", item.id);
    }
    Ok(Change {
        event: LedgerEvent::ItemReopened(Reopened {
            item_id: item.id.clone(),
            note: note.unwrap_or_default().to_owned(),
            reason: reason.to_owned(),
        }),
        receipt: format!("reopened {}", item.id),
    })
}

fn supersede_event(item: &Item, by: &str, reason: &str) -> Change {
    Change {
        event: LedgerEvent::ItemSuperseded(Superseded {
            item_id: item.id.clone(),
            by: by.to_owned(),
            reason: reason.to_owned(),
        }),
        receipt: format!("{} superseded by {by}", item.id),
    }
}

fn link_event(item: &Item, kind: &str, target: &str, reason: Option<&str>) -> Change {
    Change {
        event: LedgerEvent::ItemLinked(Linked {
            item_id: item.id.clone(),
            kind: kind.to_owned(),
            target: target.to_owned(),
            reason: reason.unwrap_or("linked").to_owned(),
        }),
        receipt: format!("linked {} → {kind} {target}", item.id),
    }
}

fn ack_event(item: &Item, reason: Option<&str>) -> Result<Change> {
    if item.status != Status::Defaulted {
        bail!(
            "{} is {}, only defaults are acknowledged",
            item.id,
            item.status.as_str()
        );
    }
    Ok(Change {
        event: LedgerEvent::ItemAcknowledged(Acknowledged {
            item_id: item.id.clone(),
            reason: reason
                .unwrap_or("left as defaulted after the digest")
                .to_owned(),
        }),
        receipt: format!("acknowledged default on {}", item.id),
    })
}

fn route_str(r: Route) -> &'static str {
    match r {
        Route::Present => "present",
        Route::Default => "default",
        Route::Dismiss => "dismiss",
    }
}

/// An operation that could not be parsed.
#[derive(Debug)]
pub(crate) struct BadOp {
    /// Whether it was tagged `"op": "raise"`: such an op still takes its `@N` slot
    /// ([`Session::skip_raise`]).
    pub(crate) raise: bool,
    /// serde's error, with what it expected.
    pub(crate) error: serde_json::Error,
}

/// Parse a JSON array (or single object) of operations.
///
/// One bad operation does not drop the others: each is parsed on its own, and its error is
/// returned in its slot.
///
/// # Errors
/// Fails only when the text is not JSON at all, with serde's error and position.
pub(crate) fn parse_ops(text: &str) -> Result<Vec<Result<Op, BadOp>>> {
    let v: serde_json::Value = serde_json::from_str(text)?;
    let arr = match v {
        serde_json::Value::Array(a) => a,
        other => vec![other],
    };
    Ok(arr
        .into_iter()
        .map(|o| {
            let raise = o.get("op").and_then(serde_json::Value::as_str) == Some("raise");
            serde_json::from_value(o).map_err(|error| BadOp { raise, error })
        })
        .collect())
}

/// Whether a path should be scanned, per config.
#[must_use]
pub(crate) fn scannable(env: &Env, rel: &str) -> bool {
    !env.config
        .scan
        .exclude
        .iter()
        .any(|p| rel.starts_with(p.as_str()))
        && !Path::new(rel).components().any(|c| c.as_os_str() == ".git")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::Config;

    const ONE_WAY: &str = "Should retention attach per record or per series?";
    const TWO_WAY: &str = "Name the helper apply or run?";

    /// An environment outside git: no tree hash, the folder is the root, the ledger beside it.
    fn env_in(dir: &Path, session: &str) -> Env {
        Env {
            cwd: dir.to_path_buf(),
            repo_root: None,
            common_dir: None,
            branch: None,
            head: None,
            worktree: None,
            session: Some(session.to_owned()),
            config: Config::default(),
            store_override: Some(dir.join("ledger.sqlite3")),
        }
    }

    /// Apply a JSON batch and return one line per op, as `weighin batch` prints them.
    fn run(s: &mut Session<'_>, batch: &serde_json::Value) -> Result<Vec<String>> {
        Ok(parse_ops(&batch.to_string())?
            .into_iter()
            .map(|op| match op {
                Ok(op) => match s.apply(&op) {
                    Ok(r) => format!("✓ {r}"),
                    Err(e) => format!("✗ {e:#}"),
                },
                Err(bad) => {
                    if bad.raise {
                        s.skip_raise();
                    }
                    format!("✗ {}", bad.error)
                }
            })
            .collect())
    }

    fn card() -> serde_json::Value {
        serde_json::json!({
            "why": "It fixes what a tenant can hold.",
            "timing": "Decide now.",
            "options": [
                {"id": "A", "title": "Per series"},
                {"id": "B", "title": "Per record"},
                {"id": "C", "title": "Both", "fails_must": "one schema"}
            ],
            "recommendation": {"option": "A"}
        })
    }

    fn id_of(q: &str) -> ItemId {
        ItemId::mint("s1", q)
    }

    /// A CRLF source file whose line asks [`ONE_WAY`], for the decision to annotate.
    fn source(dir: &Path) -> Result<()> {
        let text = format!("# Plan\r\n\r\n{ONE_WAY}\r\n- tail\r\n");
        std::fs::write(dir.join("plan.md"), text)?;
        Ok(())
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens SQLite through FFI")]
    fn raise_receipts_name_new_existing_and_linked_items() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let first = env_in(dir.path(), "s1");
        let second = env_in(dir.path(), "s2");
        let raise = serde_json::json!([{"op": "raise", "q": ONE_WAY, "door": "one-way"}]);
        let id = id_of(ONE_WAY);

        let new = run(&mut Session::open(&first, "t")?, &raise)?;
        let again = run(&mut Session::open(&first, "t")?, &raise)?;
        let elsewhere = run(&mut Session::open(&second, "t")?, &raise)?;

        assert_eq!(new, [format!("✓ raised {id} (raised)")]);
        assert_eq!(again, [format!("✓ {id} already raised")]);
        assert_eq!(elsewhere, [format!("✓ linked to open {id}")]);
        Ok(())
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens SQLite through FFI")]
    fn an_acted_two_way_judgment_call_is_raised_as_a_default() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let env = env_in(dir.path(), "s1");
        let batch = serde_json::json!([
            {"op": "raise", "q": TWO_WAY, "door": "two-way", "authority": "judgment", "did": "named it apply"},
            {"op": "ack", "id": "@1"}
        ]);

        let out = run(&mut Session::open(&env, "t")?, &batch)?;

        let id = id_of(TWO_WAY);
        assert_eq!(
            out,
            [
                format!("✓ raised {id} (defaulted)"),
                format!("✓ acknowledged default on {id}"),
            ]
        );
        Ok(())
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens SQLite through FFI")]
    fn a_sitting_prints_each_receipt_unchanged() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let env = env_in(dir.path(), "s1");
        source(dir.path())?;
        let batch = serde_json::json!([
            {"op": "raise", "q": ONE_WAY, "door": "one-way", "authority": "authority", "source_path": "plan.md"},
            {"op": "triage", "id": "@1", "door": "one-way", "blast": "system", "route": "present", "reason": "r"},
            {"op": "ground", "id": "@1", "card": card()},
            {"op": "present", "id": "@1"},
            {"op": "decide", "id": "@1", "option": "B", "seconds": 5},
            {"op": "reopen", "id": "@1", "reason": "r"},
            {"op": "supersede", "id": "@1", "by": "kb-0001", "reason": "r"},
            {"op": "link", "id": "@1", "kind": "relates", "target": "wi-000000"}
        ]);

        let out = run(
            &mut Session::open(&env, "t")?.with_today("2026-01-02"),
            &batch,
        )?;

        let id = id_of(ONE_WAY);
        let atom = format!(
            ".kb/_intake/decisions/{id}-should-retention-attach-per-record-or-per-series.md"
        );
        assert_eq!(
            out,
            [
                format!("✓ raised {id} (raised)"),
                format!("✓ triaged {id} · one-way · system · present"),
                format!("✓ grounded {id} · 3 options · recommendation earned"),
                format!("✓ presented {id} (chat)"),
                format!(
                    "✓ decided {id} → B (override) · atom {atom} · annotated plan.md · trailer `Weigh-In: {id} decided B`"
                ),
                format!("✓ reopened {id}"),
                format!("✓ {id} superseded by kb-0001"),
                format!("✓ linked {id} → relates wi-000000"),
            ]
        );
        let annotated = std::fs::read_to_string(dir.path().join("plan.md"))?;
        assert_eq!(
            annotated,
            format!(
                "# Plan\r\n\r\n{ONE_WAY} → Weigh-In {id}: decided B (2026-01-02)\r\n- tail\r\n"
            )
        );
        Ok(())
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens SQLite through FFI")]
    fn deferring_a_one_way_door_writes_a_tracking_atom() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let env = env_in(dir.path(), "s1");
        let batch = serde_json::json!([
            {"op": "raise", "q": ONE_WAY, "door": "one-way"},
            {"op": "triage", "id": "@1", "door": "one-way", "blast": "module", "route": "present", "reason": "r"},
            {"op": "defer", "id": "@1", "flip": "a tenant asks", "trigger": "the tenant survey", "resolve_by": "2026-12-01", "default": "A"}
        ]);

        let out = run(&mut Session::open(&env, "t")?, &batch)?;

        let id = id_of(ONE_WAY);
        assert_eq!(
            out.get(2),
            Some(&format!(
                "✓ deferred {id} until the tenant survey (default A by 2026-12-01) · tracking atom .kb/_intake/open_questions/{id}-should-retention-attach-per-record-or-per-series.md"
            ))
        );
        Ok(())
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens SQLite through FFI")]
    fn defaults_and_dismissals_print_their_cause() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let env = env_in(dir.path(), "s1");
        let batch = serde_json::json!([
            {"op": "raise", "q": TWO_WAY, "door": "two-way", "authority": "judgment"},
            {"op": "default", "id": "@1", "option": "apply", "cause": "two-way", "reason": "r"},
            {"op": "raise", "q": "Which colour for the badge?"},
            {"op": "dismiss", "id": "@2", "why": "moot", "reason": "r"}
        ]);

        let out = run(&mut Session::open(&env, "t")?, &batch)?;

        let two_way = id_of(TWO_WAY);
        let badge = id_of("Which colour for the badge?");
        assert_eq!(
            out,
            [
                format!("✓ raised {two_way} (raised)"),
                format!("✓ defaulted {two_way} to apply (two-way)"),
                format!("✓ raised {badge} (raised)"),
                format!("✓ dismissed {badge} (moot)"),
            ]
        );
        Ok(())
    }

    /// Run `setup`, then `ops`, in one batch; return only the lines `ops` printed, failing when a
    /// setup op was refused.
    fn after(setup: &[serde_json::Value], ops: &[serde_json::Value]) -> Result<Vec<String>> {
        let dir = tempfile::tempdir()?;
        let env = env_in(dir.path(), "s1");
        let batch: Vec<_> = setup.iter().chain(ops).cloned().collect();
        let mut out = run(
            &mut Session::open(&env, "t")?,
            &serde_json::Value::Array(batch),
        )?;
        let tail = out.split_off(setup.len().min(out.len()));
        if let Some(refused) = out.iter().find(|line| !line.starts_with('✓')) {
            anyhow::bail!("setup refused: {refused}");
        }
        Ok(tail)
    }

    /// [`ONE_WAY`], raised as a one-way door bound to authority.
    fn raised() -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({"op": "raise", "q": ONE_WAY, "door": "one-way", "authority": "authority"}),
        ]
    }

    /// [`ONE_WAY`], raised and grounded on [`card`].
    fn grounded() -> Vec<serde_json::Value> {
        let mut ops = raised();
        ops.push(serde_json::json!({"op": "ground", "id": "@1", "card": card()}));
        ops
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens SQLite through FFI")]
    fn a_raise_without_a_question_is_refused() -> Result<()> {
        let out = after(&[], &[serde_json::json!({"op": "raise", "q": "  "})])?;

        assert_eq!(out, ["✗ a raised item needs a question"]);
        Ok(())
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens SQLite through FFI")]
    fn refs_that_name_no_item_are_refused() -> Result<()> {
        let out = after(
            &raised(),
            &[
                serde_json::json!({"op": "present", "id": "@x"}),
                serde_json::json!({"op": "present", "id": "@3"}),
                serde_json::json!({"op": "present", "id": "wi-zzzzzz"}),
            ],
        )?;

        assert_eq!(
            out,
            [
                "✗ `@x`: refs are @1, @2, ... (raises earlier in this batch)",
                "✗ `@3`: only 1 raise(s) so far in this batch",
                "✗ no single item matches `wi-zzzzzz` (try `weighin open --all`)",
            ]
        );
        Ok(())
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens SQLite through FFI")]
    fn a_ref_to_a_failed_raise_is_refused_and_later_refs_keep_their_place() -> Result<()> {
        let out = after(
            &[],
            &[
                serde_json::json!({"op": "raise", "q": "   "}),
                serde_json::json!({"op": "raise", "q": ONE_WAY}),
                serde_json::json!({"op": "dismiss", "id": "@1", "why": "moot", "reason": "r"}),
                serde_json::json!({"op": "triage", "id": "@2", "door": "one-way", "blast": "file", "route": "present", "reason": "r"}),
            ],
        )?;

        let id = id_of(ONE_WAY);
        assert_eq!(
            out,
            [
                "✗ a raised item needs a question".to_owned(),
                format!("✓ raised {id} (raised)"),
                "✗ `@1`: raise 1 in this batch failed, so it names no item".to_owned(),
                format!("✓ triaged {id} · one-way · file · present"),
            ]
        );
        Ok(())
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens SQLite through FFI")]
    fn triage_refuses_unknown_values_and_defaulting_a_one_way_door() -> Result<()> {
        let out = after(
            &raised(),
            &[
                serde_json::json!({"op": "triage", "id": "@1", "door": "one-way", "blast": "galaxy", "route": "present", "reason": "r"}),
                serde_json::json!({"op": "triage", "id": "@1", "door": "one-way", "blast": "file", "route": "later", "reason": "r"}),
                serde_json::json!({"op": "triage", "id": "@1", "door": "one-way", "blast": "file", "route": "default", "reason": "r"}),
            ],
        )?;

        let id = id_of(ONE_WAY);
        assert_eq!(
            out,
            [
                "✗ blast must be file, module, system or external".to_owned(),
                "✗ route must be present, default or dismiss".to_owned(),
                format!(
                    "✗ {id}: only a judgment-bound two-way door may be defaulted (door one-way, authority)"
                ),
            ]
        );
        Ok(())
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens SQLite through FFI")]
    fn decide_refuses_an_unknown_option_and_one_that_fails_a_must() -> Result<()> {
        let out = after(
            &grounded(),
            &[
                serde_json::json!({"op": "decide", "id": "@1", "option": "Z"}),
                serde_json::json!({"op": "decide", "id": "@1", "option": "C"}),
            ],
        )?;

        let id = id_of(ONE_WAY);
        assert_eq!(
            out,
            [
                format!("✗ {id} has no option `Z` (options: A, B, C)"),
                format!("✗ {id}: option C fails a MUST (one schema)"),
            ]
        );
        Ok(())
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens SQLite through FFI")]
    fn defer_refuses_a_missing_flip_and_a_default_that_is_not_an_option() -> Result<()> {
        let out = after(
            &grounded(),
            &[
                serde_json::json!({"op": "defer", "id": "@1", "trigger": " ", "resolve_by": "soon", "default": "A"}),
                serde_json::json!({"op": "defer", "id": "@1", "flip": "f", "trigger": "t", "resolve_by": "soon", "default": "Z"}),
            ],
        )?;

        let id = id_of(ONE_WAY);
        assert_eq!(
            out,
            [
                "✗ a deferral needs flip: waiting without a named fact is the can being kicked"
                    .to_owned(),
                format!("✗ {id}: default `Z` is not an option"),
            ]
        );
        Ok(())
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens SQLite through FFI")]
    fn default_refuses_an_unknown_cause_and_a_one_way_door() -> Result<()> {
        let out = after(
            &grounded(),
            &[
                serde_json::json!({"op": "default", "id": "@1", "option": "A", "cause": "whenever", "reason": "r"}),
                serde_json::json!({"op": "default", "id": "@1", "option": "A", "cause": "two-way", "reason": "r"}),
            ],
        )?;

        let id = id_of(ONE_WAY);
        assert_eq!(
            out,
            [
                "✗ cause must be two-way, expired or unattended".to_owned(),
                format!(
                    "✗ {id} is a one-way door; only a two-way door is defaulted (triage it first)"
                ),
            ]
        );
        Ok(())
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens SQLite through FFI")]
    fn dismiss_refuses_an_unknown_why() -> Result<()> {
        let out = after(
            &grounded(),
            &[serde_json::json!({"op": "dismiss", "id": "@1", "why": "bored", "reason": "r"})],
        )?;

        assert_eq!(
            out,
            [
                "✗ why must be not-useful, recoverable, moot, already-decided:<ref>, duplicate-of:<id> or not-my-call:<who>"
            ]
        );
        Ok(())
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens SQLite through FFI")]
    fn reopen_refuses_an_open_item() -> Result<()> {
        let out = after(
            &grounded(),
            &[serde_json::json!({"op": "reopen", "id": "@1"})],
        )?;

        assert_eq!(out, [format!("✗ {} is already open", id_of(ONE_WAY))]);
        Ok(())
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens SQLite through FFI")]
    fn ack_refuses_an_item_that_was_not_defaulted() -> Result<()> {
        let out = after(&grounded(), &[serde_json::json!({"op": "ack", "id": "@1"})])?;

        assert_eq!(
            out,
            [format!(
                "✗ {} is grounded, only defaults are acknowledged",
                id_of(ONE_WAY)
            )]
        );
        Ok(())
    }
}

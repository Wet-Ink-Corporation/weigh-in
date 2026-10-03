//! Ledger operations. Every command, every hook and the `batch` entry point goes through
//! [`Session`], which is the single write path: it reads the ledger, validates, appends under a
//! guard, and performs the side effects (atoms, source annotations) that follow a decision.

use anyhow::{Context as _, Result, anyhow, bail};
use serde::Deserialize;
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

use crate::env::{self, Env};
use crate::git;
use crate::store::{Appended, Guard, Store};

/// What an agent or a person raises.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct RaiseInput {
    pub q: String,
    pub kind: Option<String>,
    pub door: Option<String>,
    pub authority: Option<String>,
    pub did: Option<String>,
    pub flip: Option<String>,
    pub at: Option<String>,
    pub sym: Option<String>,
    pub blocks: Option<String>,
    pub channel: Option<String>,
    pub quote: Option<String>,
    pub source_path: Option<String>,
    pub source_line: Option<usize>,
}

impl RaiseInput {
    #[must_use]
    pub fn from_marker(m: &Marker, channel: Channel) -> Self {
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
pub enum Op {
    Raise(RaiseInput),
    Triage {
        id: String,
        door: String,
        blast: String,
        #[serde(default)]
        authority: Option<String>,
        route: String,
        #[serde(default)]
        timing: Option<Timing>,
        #[serde(default)]
        resolve_by: Option<String>,
        reason: String,
    },
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
    Decide {
        id: String,
        option: String,
        #[serde(default)]
        note: Option<String>,
        #[serde(default)]
        provenance: Option<String>,
        #[serde(default)]
        via: Option<String>,
        #[serde(default)]
        seconds: Option<u64>,
    },
    Defer {
        id: String,
        #[serde(default)]
        flip: String,
        trigger: String,
        resolve_by: String,
        default: String,
        #[serde(default)]
        owner: Option<String>,
        #[serde(default)]
        unqualified: bool,
        #[serde(default)]
        reason: String,
    },
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
pub enum RaiseOutcome {
    New(ItemId),
    /// The same session raised it before (a hook firing twice).
    Existing(ItemId),
    /// Another open item already asks this; linked to it instead.
    Duplicate(ItemId),
}

impl RaiseOutcome {
    #[must_use]
    pub fn id(&self) -> &ItemId {
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
    let path = env.store_path();
    if path
        .try_exists()
        .with_context(|| format!("checking {}", path.display()))?
    {
        Store::open(&path)?.ledger().map(Some)
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

pub struct Session<'a> {
    pub env: &'a Env,
    pub store: Store,
    pub ledger: Ledger,
    tree: Option<Option<String>>,
    actor: String,
    /// Ids raised earlier in this batch, so later ops can say `@1`, `@2`, ...
    raised: Vec<ItemId>,
}

impl<'a> Session<'a> {
    /// # Errors
    /// Fails when the store cannot be opened or read.
    pub fn open(env: &'a Env, actor: &str) -> Result<Self> {
        let store = Store::open(&env.store_path())?;
        let ledger = store.ledger()?;
        Ok(Self {
            env,
            store,
            ledger,
            tree: None,
            actor: actor.to_owned(),
            raised: Vec::new(),
        })
    }

    fn tree_hash(&mut self) -> Option<String> {
        if self.tree.is_none() {
            let t = self.env.repo_root.as_ref().and_then(|r| git::tree_hash(r));
            self.tree = Some(t);
        }
        self.tree.clone().flatten()
    }

    /// Append events for `id` guarded on the item being unchanged since this session read it.
    fn write(&mut self, id: &ItemId, events: Vec<LedgerEvent>) -> Result<()> {
        let since = self.ledger.get(id).map_or(0, |i| i.last_position);
        let envs: Vec<_> = events
            .into_iter()
            .map(|e| self.env.envelope(e, &self.actor))
            .collect();
        match self
            .store
            .append(&envs, &Guard::ItemUnchangedSince(id.clone(), since))?
        {
            Appended::At(last) => {
                let first = last + 1 - envs.len() as u64;
                for (i, e) in envs.iter().enumerate() {
                    self.ledger.apply(first + i as u64, e);
                }
                Ok(())
            }
            Appended::Conflict => {
                self.ledger = self.store.ledger()?;
                bail!(
                    "{id} changed while this was being written (another session acted on it); re-read with `weighin show {id}` and retry"
                )
            }
        }
    }

    fn item(&self, needle: &str) -> Result<Item> {
        if let Some(n) = needle.strip_prefix('@') {
            let k: usize = n.parse().map_err(|_| {
                anyhow!("`{needle}`: refs are @1, @2, ... (raises earlier in this batch)")
            })?;
            let id = self.raised.get(k.wrapping_sub(1)).ok_or_else(|| {
                anyhow!(
                    "`{needle}`: only {} raise(s) so far in this batch",
                    self.raised.len()
                )
            })?;
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
    pub fn raise(&mut self, input: &RaiseInput) -> Result<RaiseOutcome> {
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
        if let Some(closed) = self
            .ledger
            .any_with_question(q)
            .filter(|i| !i.status.is_open())
        {
            // Settled (decided, defaulted, dismissed): never re-asked. Record the re-raise only.
            let cid = closed.id.clone();
            if closed.session_id.as_deref() != Some(session.as_str())
                && !closed.also_raised_in.contains(&session)
            {
                self.write(
                    &cid,
                    vec![LedgerEvent::ItemLinked(Linked {
                        item_id: cid.clone(),
                        kind: "raised-again".into(),
                        target: session,
                        reason: "a settled question was raised again; not re-asked".into(),
                    })],
                )?;
            }
            return Ok(RaiseOutcome::Existing(cid));
        }
        if let Some(existing) = self.ledger.open_with_question(q) {
            let eid = existing.id.clone();
            if eid == minted || existing.session_id.as_deref() == Some(session.as_str()) {
                return Ok(RaiseOutcome::Existing(eid));
            }
            if !existing.also_raised_in.contains(&session) {
                self.write(
                    &eid,
                    vec![LedgerEvent::ItemLinked(Linked {
                        item_id: eid.clone(),
                        kind: "raised-again".into(),
                        target: session,
                        reason: "the same question was raised in another session".into(),
                    })],
                )?;
            }
            return Ok(RaiseOutcome::Duplicate(eid));
        }
        let channel = match input.channel.as_deref() {
            Some("scan") => Channel::Scan,
            Some("sweep") => Channel::Sweep,
            Some("manual") => Channel::Manual,
            _ => Channel::Marker,
        };
        let door = input.door.as_deref().map(Door::parse).unwrap_or_default();
        let authority = input
            .authority
            .as_deref()
            .map(Authority::parse)
            .unwrap_or_default();
        let mk = |id: ItemId| {
            LedgerEvent::ItemRaised(Raised {
                item_id: id,
                kind: input.kind.as_deref().map(Kind::parse).unwrap_or_default(),
                question: q.to_owned(),
                did: input.did.clone(),
                flip: input.flip.clone(),
                at: input.at.as_deref().map(|a| self.env.relative_anchor(a)),
                sym: input.sym.clone(),
                blocks: input.blocks.clone(),
                door_hint: door,
                authority_hint: authority,
                channel,
                quote: input.quote.clone().unwrap_or_else(|| q.to_owned()),
                source_path: input.source_path.clone(),
                source_line: input.source_line,
            })
        };
        let mut id = minted;
        for attempt in 0..2 {
            let env = self.env.envelope(mk(id.clone()), &self.actor);
            match self
                .store
                .append(std::slice::from_ref(&env), &Guard::NotYetRaised(id.clone()))?
            {
                Appended::At(p) => {
                    self.ledger.apply(p, &env);
                    break;
                }
                Appended::Conflict => {
                    self.ledger = self.store.ledger()?;
                    let same = self.ledger.get(&id).is_some_and(|i| {
                        weighin_core::id::question_key(&i.question)
                            == weighin_core::id::question_key(q)
                    });
                    if same {
                        return Ok(RaiseOutcome::Existing(id));
                    }
                    if attempt == 1 {
                        bail!("could not mint a free id for this question");
                    }
                    id = ItemId::mint_with_len(&session, q, EXTENDED_HEX_LEN);
                }
            }
        }
        // Decision wi-4d8b6a: a judgment-bound two-way door the agent already acted on is its own
        // call, logged as a default and listed in the digest; it never becomes a question.
        let did = input
            .did
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty() && !d.eq_ignore_ascii_case("blocked"));
        // The tripwire keeps authority-sounding questions (compliance, isolation, retention, ...)
        // open for the sitting even when the agent called them judgment calls.
        let tripwire = authority_tripwire(&format!(
            "{q} {} {}",
            did.unwrap_or(""),
            input.flip.as_deref().unwrap_or("")
        ));
        if channel == Channel::Marker
            && door == Door::TwoWay
            && authority == Authority::Judgment
            && tripwire.is_none()
            && let Some(did) = did
        {
            self.write(&id, vec![LedgerEvent::ItemDefaulted(Defaulted { item_id: id.clone(), option: did.to_owned(), cause: DefaultCause::TwoWay, reason: "judgment-bound two-way door the agent acted on; logged, reversible from the digest".into() })])?;
        }
        Ok(RaiseOutcome::New(id))
    }

    /// Apply one operation and return a one-line receipt.
    ///
    /// # Errors
    /// Fails when the operation is invalid for the item or the store refuses it.
    pub fn apply(&mut self, op: &Op) -> Result<String> {
        match op {
            Op::Raise(r) => {
                let out = self.raise(r)?;
                self.raised.push(out.id().clone());
                let status = self.ledger.get(out.id()).map_or("?", |i| i.status.as_str());
                Ok(match out {
                    RaiseOutcome::New(id) => format!("raised {id} ({status})"),
                    RaiseOutcome::Existing(id) => format!("{id} already raised"),
                    RaiseOutcome::Duplicate(id) => format!("linked to open {id}"),
                })
            }
            Op::Triage {
                id,
                door,
                blast,
                authority,
                route,
                timing,
                resolve_by,
                reason,
            } => {
                let item = self.item(id)?;
                let blast = Blast::parse(blast)
                    .ok_or_else(|| anyhow!("blast must be file, module, system or external"))?;
                let route = match route.as_str() {
                    "present" => Route::Present,
                    "default" => Route::Default,
                    "dismiss" => Route::Dismiss,
                    _ => bail!("route must be present, default or dismiss"),
                };
                let door = Door::parse(door);
                let authority = authority
                    .as_deref()
                    .map_or(item.authority(), Authority::parse);
                if route == Route::Default
                    && (door != Door::TwoWay || authority != Authority::Judgment)
                {
                    bail!(
                        "{}: only a judgment-bound two-way door may be defaulted (door {}, {})",
                        item.id,
                        door.as_str(),
                        authority.as_str()
                    );
                }
                self.write(
                    &item.id,
                    vec![LedgerEvent::ItemTriaged(Triaged {
                        item_id: item.id.clone(),
                        door,
                        blast,
                        authority,
                        route,
                        timing: timing.clone(),
                        resolve_by: resolve_by.clone(),
                        reason: reason.clone(),
                    })],
                )?;
                Ok(format!(
                    "triaged {} · {} · {} · {}",
                    item.id,
                    door.as_str(),
                    blast.as_str(),
                    route_str(route)
                ))
            }
            Op::Ground { id, card, reason } => {
                let item = self.item(id)?;
                card.validate().map_err(|e| anyhow!("{}: {e}", item.id))?;
                self.write(
                    &item.id,
                    vec![LedgerEvent::ItemGrounded(Grounded {
                        item_id: item.id.clone(),
                        card: card.clone(),
                        reason: reason.clone().unwrap_or_else(|| "grounded".into()),
                    })],
                )?;
                let earned = if card.recommendation_earned() {
                    "recommendation earned"
                } else {
                    "options first"
                };
                Ok(format!(
                    "grounded {} · {} options · {earned}",
                    item.id,
                    card.options.len()
                ))
            }
            Op::Present { id, surface } => {
                let item = self.item(id)?;
                if !item.status.is_open() {
                    bail!("{} is {}, not open", item.id, item.status.as_str());
                }
                let tree = self.tree_hash();
                let surface = surface.clone().unwrap_or_else(|| "chat".into());
                self.write(
                    &item.id,
                    vec![LedgerEvent::ItemPresented(Presented {
                        item_id: item.id.clone(),
                        surface: surface.clone(),
                        tree_hash: tree,
                        reason: format!("shown in {surface}"),
                    })],
                )?;
                Ok(format!("presented {} ({surface})", item.id))
            }
            Op::Decide {
                id,
                option,
                note,
                provenance,
                via,
                seconds,
            } => self.decide(
                id,
                option,
                note.as_deref().unwrap_or(""),
                provenance.as_deref(),
                via.as_deref().unwrap_or("chat"),
                *seconds,
            ),
            Op::Defer {
                id,
                flip,
                trigger,
                resolve_by,
                default,
                owner,
                unqualified,
                reason,
            } => {
                let item = self.item(id)?;
                if !item.status.is_open() {
                    bail!("{} is {}, not open", item.id, item.status.as_str());
                }
                if let Some(card) = &item.card
                    && card.option(default).is_none()
                {
                    bail!("{}: default `{default}` is not an option", item.id);
                }
                // A bare "defer" from the user is recorded as unqualified: it names no flip fact
                // and is counted against the action rate.
                for (k, v) in [
                    ("flip", if *unqualified { trigger } else { flip }),
                    ("trigger", trigger),
                    ("resolve_by", resolve_by),
                ] {
                    if v.trim().is_empty() {
                        bail!(
                            "a deferral needs {k}: waiting without a named fact is the can being kicked"
                        );
                    }
                }
                let owner = owner
                    .clone()
                    .unwrap_or_else(|| self.env.config.owner.clone());
                self.write(
                    &item.id,
                    vec![LedgerEvent::ItemDeferred(Deferred {
                        item_id: item.id.clone(),
                        flip: flip.clone(),
                        trigger: trigger.clone(),
                        resolve_by: resolve_by.clone(),
                        default_option: default.clone(),
                        owner,
                        unqualified: *unqualified,
                        reason: reason.clone(),
                    })],
                )?;
                let mut receipt = format!(
                    "deferred {} until {trigger} (default {default} by {resolve_by})",
                    item.id
                );
                if item.door() == Door::OneWay
                    && let Some(path) = self.write_open_question(&item.id)?
                {
                    receipt.push_str(&format!(" · tracking atom {path}"));
                }
                Ok(receipt)
            }
            Op::Default {
                id,
                option,
                cause,
                reason,
            } => {
                let item = self.item(id)?;
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
                self.write(
                    &item.id,
                    vec![LedgerEvent::ItemDefaulted(Defaulted {
                        item_id: item.id.clone(),
                        option: option.clone(),
                        cause,
                        reason: reason.clone(),
                    })],
                )?;
                Ok(format!(
                    "defaulted {} to {option} ({})",
                    item.id,
                    cause.as_str()
                ))
            }
            Op::Dismiss {
                id,
                why,
                note,
                reason,
            } => {
                let item = self.item(id)?;
                let why = DismissReason::parse(why).ok_or_else(|| anyhow!("why must be not-useful, recoverable, moot, already-decided:<ref>, duplicate-of:<id> or not-my-call:<who>"))?;
                // Papering-over guard: an authority-sounding item is "already decided" only by an
                // accepted .kb decision or by the user's own words choosing it.
                if let DismissReason::AlreadyDecided(r) = &why
                    && let Some(word) =
                        authority_tripwire(&format!("{} {}", item.question, item.quote))
                    && !(r.starts_with("kb-") || r.starts_with("chat:"))
                {
                    bail!(
                        "{} is authority-bound (`{word}`): cite the accepted atom (already-decided:kb-…) or the user's words choosing it (already-decided:chat:\"…\"). \"Assume and keep going\" delegates the work, not this decision; present it instead",
                        item.id
                    );
                }
                self.write(
                    &item.id,
                    vec![LedgerEvent::ItemDismissed(Dismissed {
                        item_id: item.id.clone(),
                        why: why.clone(),
                        note: note.clone().unwrap_or_default(),
                        reason: reason.clone(),
                    })],
                )?;
                Ok(format!("dismissed {} ({})", item.id, why.label()))
            }
            Op::Reopen { id, note, reason } => {
                let item = self.item(id)?;
                if item.status.is_open() && item.status != Status::Deferred {
                    bail!("{} is already open", item.id);
                }
                self.write(
                    &item.id,
                    vec![LedgerEvent::ItemReopened(Reopened {
                        item_id: item.id.clone(),
                        note: note.clone().unwrap_or_default(),
                        reason: reason.clone(),
                    })],
                )?;
                Ok(format!("reopened {}", item.id))
            }
            Op::Supersede { id, by, reason } => {
                let item = self.item(id)?;
                self.write(
                    &item.id,
                    vec![LedgerEvent::ItemSuperseded(Superseded {
                        item_id: item.id.clone(),
                        by: by.clone(),
                        reason: reason.clone(),
                    })],
                )?;
                Ok(format!("{} superseded by {by}", item.id))
            }
            Op::Link {
                id,
                kind,
                target,
                reason,
            } => {
                let item = self.item(id)?;
                self.write(
                    &item.id,
                    vec![LedgerEvent::ItemLinked(Linked {
                        item_id: item.id.clone(),
                        kind: kind.clone(),
                        target: target.clone(),
                        reason: reason.clone().unwrap_or_else(|| "linked".into()),
                    })],
                )?;
                Ok(format!("linked {} → {kind} {target}", item.id))
            }
            Op::Ack { id, reason } => {
                let item = self.item(id)?;
                if item.status != Status::Defaulted {
                    bail!(
                        "{} is {}, only defaults are acknowledged",
                        item.id,
                        item.status.as_str()
                    );
                }
                self.write(
                    &item.id,
                    vec![LedgerEvent::ItemAcknowledged(Acknowledged {
                        item_id: item.id.clone(),
                        reason: reason
                            .clone()
                            .unwrap_or_else(|| "left as defaulted after the digest".into()),
                    })],
                )?;
                Ok(format!("acknowledged default on {}", item.id))
            }
        }
    }

    fn decide(
        &mut self,
        id: &str,
        option: &str,
        note: &str,
        provenance: Option<&str>,
        via: &str,
        seconds: Option<u64>,
    ) -> Result<String> {
        let item = self.item(id)?;
        if !item.status.is_open() {
            bail!(
                "{} is {}, not open; reopen it first",
                item.id,
                item.status.as_str()
            );
        }
        let matched = item
            .card
            .as_ref()
            .map(|c| c.recommendation.option == option);
        if let Some(card) = &item.card {
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
        }
        let provenance = match provenance {
            Some("user-approved") => Provenance::UserApproved,
            Some("user-directed") => Provenance::UserDirected,
            _ if matched == Some(true) => Provenance::UserApproved,
            _ => Provenance::UserDirected,
        };
        let now = env::now();
        let seconds = seconds.or_else(|| {
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
                option: option.to_owned(),
                decider,
                provenance,
                note: note.to_owned(),
                tree_hash: tree,
                matched_recommendation: matched,
                via: via.to_owned(),
                seconds_to_decide: seconds,
                reason: match matched {
                    Some(true) => "chose the recommended option".into(),
                    Some(false) => "chose against the recommendation".into(),
                    None => "answered".into(),
                },
            })],
        )?;
        let mut receipt = format!("decided {} → {option}", item.id);
        if matched == Some(false) {
            receipt.push_str(" (override)");
        }
        if let Some(path) = self.write_decision_atom(&item.id)? {
            receipt.push_str(&format!(" · atom {path}"));
        }
        if let Some(src) = self.annotate_source(&item.id, option)? {
            receipt.push_str(&format!(" · annotated {src}"));
        }
        receipt.push_str(&format!(
            " · trailer `Weigh-In: {} decided {option}`",
            item.id
        ));
        Ok(receipt)
    }

    fn render_ctx(&self) -> RenderContext {
        RenderContext {
            date: env::today(),
            depends_on: Vec::new(),
            source_paths: Vec::new(),
        }
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
        let Some(text) = atom::render_decision(&item, &self.render_ctx()) else {
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
        let Some(text) = atom::render_open_question(&item, &self.render_ctx()) else {
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

    /// spec-kit's write-back: the line the question came from gets an arrow, so it is not raised again.
    fn annotate_source(&self, id: &ItemId, option: &str) -> Result<Option<String>> {
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
        // Rebuild with each line's own terminator so a CRLF file stays CRLF.
        let mut done = false;
        let mut joined = String::with_capacity(text.len() + 64);
        for chunk in text.split_inclusive('\n') {
            let (body, end) = match chunk.strip_suffix("\r\n") {
                Some(b) => (b, "\r\n"),
                None => match chunk.strip_suffix('\n') {
                    Some(b) => (b, "\n"),
                    None => (chunk, ""),
                },
            };
            if !done && body.trim() == first && !body.contains("→ Weigh-In") {
                done = true;
                joined.push_str(&format!(
                    "{body} → Weigh-In {id}: decided {option} ({})",
                    env::today()
                ));
            } else {
                joined.push_str(body);
            }
            joined.push_str(end);
        }
        if !done {
            return Ok(None);
        }
        std::fs::write(&path, joined)?;
        Ok(Some(src.clone()))
    }
}

fn route_str(r: Route) -> &'static str {
    match r {
        Route::Present => "present",
        Route::Default => "default",
        Route::Dismiss => "dismiss",
    }
}

/// Parse a JSON array (or single object) of operations.
///
/// # Errors
/// Returns the serde error with the offending position.
/// One bad operation does not drop the others: each is parsed on its own.
///
/// # Errors
/// Fails only when the text is not JSON at all.
pub fn parse_ops(text: &str) -> Result<Vec<Result<Op>>> {
    let v: serde_json::Value = serde_json::from_str(text)?;
    let arr = match v {
        serde_json::Value::Array(a) => a,
        other => vec![other],
    };
    Ok(arr
        .into_iter()
        .map(|o| serde_json::from_value(o).map_err(|e| anyhow!("{e}")))
        .collect())
}

/// Whether a path should be scanned, per config.
#[must_use]
pub fn scannable(env: &Env, rel: &str) -> bool {
    !env.config
        .scan
        .exclude
        .iter()
        .any(|p| rel.starts_with(p.as_str()))
        && !Path::new(rel).components().any(|c| c.as_os_str() == ".git")
}

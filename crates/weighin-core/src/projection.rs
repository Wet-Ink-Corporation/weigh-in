//! Current state as a fold of events. The store's position is the only ordering key; a caller
//! folds events in position order and nothing here trusts timestamps for order.

use crate::card::Card;
use crate::event::{
    Blast, Channel, DefaultCause, Deferred, DismissReason, EventEnvelope, LedgerEvent, Provenance,
    Route, Timing,
};
use crate::id::{ItemId, question_key};
use crate::marker::{Authority, Door, Kind};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Raised,
    Triaged,
    Grounded,
    Presented,
    Deferred,
    Decided,
    Defaulted,
    Dismissed,
    Superseded,
}

impl Status {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Raised => "raised",
            Self::Triaged => "triaged",
            Self::Grounded => "grounded",
            Self::Presented => "presented",
            Self::Deferred => "deferred",
            Self::Decided => "decided",
            Self::Defaulted => "defaulted",
            Self::Dismissed => "dismissed",
            Self::Superseded => "superseded",
        }
    }
    /// Still waiting on someone (a deferred item waits on its trigger).
    #[must_use]
    pub fn is_open(self) -> bool {
        matches!(
            self,
            Self::Raised | Self::Triaged | Self::Grounded | Self::Presented | Self::Deferred
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Decision {
    pub option: String,
    pub decider: String,
    pub provenance: Provenance,
    pub note: String,
    pub via: String,
    pub matched_recommendation: Option<bool>,
    pub tree_hash: Option<String>,
    pub seconds_to_decide: Option<u64>,
    pub ts: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DefaultTaken {
    pub option: String,
    pub cause: DefaultCause,
    pub reason: String,
    pub ts: String,
    pub acknowledged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Record {
    pub path: String,
    pub atom_id: String,
    pub atom_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Item {
    pub id: ItemId,
    pub kind: Kind,
    pub question: String,
    pub question_key: String,
    pub did: Option<String>,
    pub flip_hint: Option<String>,
    pub at: Option<String>,
    pub sym: Option<String>,
    pub blocks: Option<String>,
    pub door_hint: Door,
    pub authority_hint: Authority,
    pub channel: Channel,
    pub quote: String,
    pub source_path: Option<String>,
    pub source_line: Option<usize>,
    pub session_id: Option<String>,
    pub worktree: Option<String>,
    pub branch: Option<String>,
    pub raised_ts: String,
    pub raised_position: u64,

    pub door: Option<Door>,
    pub blast: Option<Blast>,
    pub authority: Option<Authority>,
    pub route: Option<Route>,
    pub timing: Option<Timing>,
    pub resolve_by: Option<String>,
    pub card: Option<Card>,
    /// Position of the grounding that produced `card`; answers must name it.
    pub card_rev: Option<u64>,
    pub presented_ts: Option<String>,
    pub presented_tree: Option<String>,
    pub times_presented: u32,

    pub status: Status,
    pub decision: Option<Decision>,
    pub deferral: Option<Deferred>,
    pub default: Option<DefaultTaken>,
    pub dismissed: Option<DismissReason>,
    pub superseded_by: Option<String>,
    pub records: Vec<Record>,
    pub links: Vec<(String, String)>,
    pub also_raised_in: Vec<String>,
    pub reopened: u32,
    pub last_position: u64,
    pub last_ts: String,
}

impl Item {
    /// Door as triaged, else as the agent hinted.
    #[must_use]
    pub fn door(&self) -> Door {
        self.door.unwrap_or(self.door_hint)
    }

    #[must_use]
    pub fn authority(&self) -> Authority {
        self.authority.unwrap_or(self.authority_hint)
    }

    /// Open and not parked behind a deferral.
    #[must_use]
    pub fn needs_attention(&self) -> bool {
        self.status.is_open() && self.status != Status::Deferred
    }

    /// Rank key: one-way doors first, then items that block others, then authority-bound, then
    /// oldest first ("worst first" within a tier, then age).
    #[must_use]
    pub fn rank_key(&self) -> (u8, u8, u8, u64) {
        let door = match self.door() {
            Door::OneWay => 0,
            Door::Unsure => 1,
            Door::TwoWay => 2,
        };
        let blocks = u8::from(self.blocks.is_none());
        let auth = u8::from(self.authority() == Authority::Judgment);
        (door, blocks, auth, self.raised_position)
    }
}

/// The folded ledger.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Ledger {
    pub items: BTreeMap<ItemId, Item>,
    pub head: u64,
    /// Events the fold could not apply (an event for an item never raised). Surfaced, not dropped.
    pub orphans: Vec<(u64, String)>,
}

impl Ledger {
    /// Apply one event at `position`. Positions must be supplied in ascending order.
    pub fn apply(&mut self, position: u64, env: &EventEnvelope) {
        self.head = self.head.max(position);
        let ts = env.ts.clone();
        if let LedgerEvent::ItemRaised(r) = &env.event {
            if let Some(existing) = self.items.get_mut(&r.item_id) {
                // Same id again: the writer's idempotency guard should prevent this; keep the first.
                if let Some(s) = &env.session_id
                    && existing.session_id.as_deref() != Some(s.as_str())
                    && !existing.also_raised_in.contains(s)
                {
                    existing.also_raised_in.push(s.clone());
                }
                return;
            }
            let item = Item {
                id: r.item_id.clone(),
                kind: r.kind,
                question: r.question.clone(),
                question_key: question_key(&r.question),
                did: r.did.clone(),
                flip_hint: r.flip.clone(),
                at: r.at.clone(),
                sym: r.sym.clone(),
                blocks: r.blocks.clone(),
                door_hint: r.door_hint,
                authority_hint: r.authority_hint,
                channel: r.channel,
                quote: r.quote.clone(),
                source_path: r.source_path.clone(),
                source_line: r.source_line,
                session_id: env.session_id.clone(),
                worktree: env.worktree.clone(),
                branch: env.branch.clone(),
                raised_ts: ts.clone(),
                raised_position: position,
                door: None,
                blast: None,
                authority: None,
                route: None,
                timing: None,
                resolve_by: None,
                card: None,
                card_rev: None,
                presented_ts: None,
                presented_tree: None,
                times_presented: 0,
                status: Status::Raised,
                decision: None,
                deferral: None,
                default: None,
                dismissed: None,
                superseded_by: None,
                records: Vec::new(),
                links: Vec::new(),
                also_raised_in: Vec::new(),
                reopened: 0,
                last_position: position,
                last_ts: ts,
            };
            self.items.insert(r.item_id.clone(), item);
            return;
        }
        let id = env.event.item_id().clone();
        let Some(item) = self.items.get_mut(&id) else {
            self.orphans.push((
                position,
                format!("{} for unknown {id}", env.event.event_type()),
            ));
            return;
        };
        item.last_position = position;
        item.last_ts.clone_from(&ts);
        match &env.event {
            LedgerEvent::ItemRaised(_) => unreachable!("handled above"),
            LedgerEvent::ItemTriaged(t) => {
                item.door = Some(t.door);
                item.blast = Some(t.blast);
                item.authority = Some(t.authority);
                item.route = Some(t.route);
                item.timing.clone_from(&t.timing);
                item.resolve_by.clone_from(&t.resolve_by);
                if item.status.is_open() && item.status < Status::Triaged {
                    item.status = Status::Triaged;
                }
            }
            LedgerEvent::ItemGrounded(g) => {
                item.card = Some(g.card.clone());
                item.card_rev = Some(position);
                if item.status.is_open() && item.status < Status::Grounded {
                    item.status = Status::Grounded;
                }
            }
            LedgerEvent::ItemPresented(p) => {
                item.presented_ts = Some(ts);
                item.presented_tree.clone_from(&p.tree_hash);
                item.times_presented += 1;
                if item.status.is_open() && item.status != Status::Deferred {
                    item.status = Status::Presented;
                }
            }
            LedgerEvent::ItemDecided(d) => {
                item.status = Status::Decided;
                item.deferral = None;
                item.decision = Some(Decision {
                    option: d.option.clone(),
                    decider: d.decider.clone(),
                    provenance: d.provenance,
                    note: d.note.clone(),
                    via: d.via.clone(),
                    matched_recommendation: d.matched_recommendation,
                    tree_hash: d.tree_hash.clone(),
                    seconds_to_decide: d.seconds_to_decide,
                    ts,
                });
            }
            LedgerEvent::ItemDeferred(d) => {
                item.status = Status::Deferred;
                item.deferral = Some(d.clone());
            }
            LedgerEvent::ItemDefaulted(d) => {
                item.status = Status::Defaulted;
                item.default = Some(DefaultTaken {
                    option: d.option.clone(),
                    cause: d.cause,
                    reason: d.reason.clone(),
                    ts,
                    acknowledged: false,
                });
            }
            LedgerEvent::ItemDismissed(d) => {
                item.status = Status::Dismissed;
                item.dismissed = Some(d.why.clone());
            }
            LedgerEvent::ItemReopened(_) => {
                item.reopened += 1;
                item.decision = None;
                item.default = None;
                item.dismissed = None;
                item.deferral = None;
                item.status = if item.card.is_some() {
                    Status::Grounded
                } else {
                    Status::Raised
                };
            }
            LedgerEvent::ItemSuperseded(s) => {
                item.status = Status::Superseded;
                item.superseded_by = Some(s.by.clone());
            }
            LedgerEvent::ItemLinked(l) => {
                if l.kind == "raised-again" {
                    if !item.also_raised_in.contains(&l.target) {
                        item.also_raised_in.push(l.target.clone());
                    }
                } else {
                    item.links.push((l.kind.clone(), l.target.clone()));
                }
            }
            LedgerEvent::RecordWritten(r) => {
                item.records.push(Record {
                    path: r.path.clone(),
                    atom_id: r.atom_id.clone(),
                    atom_kind: r.atom_kind.clone(),
                });
            }
            LedgerEvent::ItemAcknowledged(_) => {
                if let Some(d) = &mut item.default {
                    d.acknowledged = true;
                }
            }
        }
    }

    /// Fold a whole ordered stream.
    #[must_use]
    pub fn fold<'a>(events: impl IntoIterator<Item = (u64, &'a EventEnvelope)>) -> Self {
        let mut l = Self::default();
        for (p, e) in events {
            l.apply(p, e);
        }
        l
    }

    /// Open items, ranked.
    #[must_use]
    pub fn open(&self) -> Vec<&Item> {
        let mut v: Vec<&Item> = self.items.values().filter(|i| i.status.is_open()).collect();
        v.sort_by_key(|i| i.rank_key());
        v
    }

    /// Open items raised in `session` (or raised again there).
    #[must_use]
    pub fn open_in_session(&self, session: &str) -> Vec<&Item> {
        self.open()
            .into_iter()
            .filter(|i| {
                i.session_id.as_deref() == Some(session)
                    || i.also_raised_in.iter().any(|s| s == session)
            })
            .collect()
    }

    /// An open item already asking this question, in any session.
    #[must_use]
    pub fn open_with_question(&self, question: &str) -> Option<&Item> {
        let k = question_key(question);
        self.items
            .values()
            .find(|i| i.status.is_open() && i.question_key == k)
    }

    /// Any item, open or closed, asking this question (a settled question is never re-raised).
    #[must_use]
    pub fn any_with_question(&self, question: &str) -> Option<&Item> {
        let k = question_key(question);
        self.items
            .values()
            .filter(|i| i.question_key == k)
            .max_by_key(|i| i.last_position)
    }

    /// Defaults nobody has looked at yet (the digest).
    #[must_use]
    pub fn unacknowledged_defaults(&self) -> Vec<&Item> {
        let mut v: Vec<&Item> = self
            .items
            .values()
            .filter(|i| {
                i.status == Status::Defaulted && i.default.as_ref().is_some_and(|d| !d.acknowledged)
            })
            .collect();
        v.sort_by_key(|i| i.rank_key());
        v
    }

    #[must_use]
    pub fn get(&self, id: &ItemId) -> Option<&Item> {
        self.items.get(id)
    }

    /// Resolve a possibly abbreviated id (`3fa9`, `wi-3fa9`) to exactly one item.
    #[must_use]
    pub fn resolve(&self, needle: &str) -> Option<&Item> {
        let n = needle.trim();
        let n = if n.starts_with("wi-") {
            n.to_owned()
        } else {
            format!("wi-{n}")
        };
        if let Some(exact) = self.items.values().find(|i| i.id.as_str() == n) {
            return Some(exact);
        }
        let mut hits = self
            .items
            .values()
            .filter(|i| i.id.as_str().starts_with(&n));
        let first = hits.next()?;
        if hits.next().is_some() {
            None
        } else {
            Some(first)
        }
    }
}

/// Health numbers over presented items, as the board shows them.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Health {
    pub presented: usize,
    pub acted: usize,
    pub not_useful: usize,
    pub chosen_against_recommendation: usize,
    pub chosen_with_recommendation_known: usize,
    pub median_seconds_to_decide: Option<u64>,
    pub deferred_unqualified: usize,
    pub defaults_unacknowledged: usize,
}

impl Health {
    #[must_use]
    pub fn of(ledger: &Ledger) -> Self {
        let mut h = Self::default();
        let mut secs = Vec::new();
        for i in ledger.items.values() {
            if i.times_presented == 0 {
                if i.status == Status::Defaulted
                    && i.default.as_ref().is_some_and(|d| !d.acknowledged)
                {
                    h.defaults_unacknowledged += 1;
                }
                continue;
            }
            h.presented += 1;
            match i.status {
                Status::Decided => {
                    h.acted += 1;
                    if let Some(d) = &i.decision {
                        if let Some(m) = d.matched_recommendation {
                            h.chosen_with_recommendation_known += 1;
                            if !m {
                                h.chosen_against_recommendation += 1;
                            }
                        }
                        if let Some(s) = d.seconds_to_decide {
                            secs.push(s);
                        }
                    }
                }
                Status::Deferred => {
                    if i.deferral.as_ref().is_some_and(|d| !d.unqualified) {
                        h.acted += 1;
                    } else {
                        h.deferred_unqualified += 1;
                    }
                }
                Status::Dismissed => match &i.dismissed {
                    Some(DismissReason::NotUseful) => h.not_useful += 1,
                    Some(DismissReason::NotMyCall(_)) => h.acted += 1,
                    _ => {}
                },
                _ => {}
            }
        }
        secs.sort_unstable();
        h.median_seconds_to_decide = secs
            .get(secs.len().saturating_sub(1) / 2)
            .copied()
            .filter(|_| !secs.is_empty());
        h
    }

    #[must_use]
    pub fn action_rate(&self) -> Option<f64> {
        (self.presented > 0).then(|| self.acted as f64 / self.presented as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::*;

    fn env(event: LedgerEvent, session: &str) -> EventEnvelope {
        EventEnvelope {
            event,
            actor: "test".into(),
            session_id: Some(session.into()),
            worktree: None,
            branch: None,
            head: None,
            ts: "2026-09-26T00:00:00Z".into(),
        }
    }

    fn raised(id: &ItemId, q: &str, door: Door) -> LedgerEvent {
        LedgerEvent::ItemRaised(Raised {
            item_id: id.clone(),
            kind: Kind::Question,
            question: q.into(),
            did: None,
            flip: None,
            at: None,
            sym: None,
            blocks: None,
            door_hint: door,
            authority_hint: Authority::Authority,
            channel: Channel::Marker,
            quote: q.into(),
            source_path: None,
            source_line: None,
        })
    }

    #[test]
    fn lifecycle_and_ranking() {
        let a = ItemId::mint("s1", "two-way thing?");
        let b = ItemId::mint("s1", "one-way thing?");
        let evs = [
            env(raised(&a, "two-way thing?", Door::TwoWay), "s1"),
            env(raised(&b, "one-way thing?", Door::OneWay), "s1"),
            env(
                LedgerEvent::ItemPresented(Presented {
                    item_id: b.clone(),
                    surface: "chat".into(),
                    tree_hash: Some("t1".into()),
                    reason: "sitting".into(),
                }),
                "s1",
            ),
            env(
                LedgerEvent::ItemDecided(Decided {
                    item_id: b.clone(),
                    option: "A".into(),
                    decider: "human:ryan".into(),
                    provenance: Provenance::UserApproved,
                    note: String::new(),
                    tree_hash: Some("t1".into()),
                    matched_recommendation: Some(true),
                    via: "chat".into(),
                    seconds_to_decide: Some(40),
                    reason: "picked the recommendation".into(),
                }),
                "s1",
            ),
        ];
        let l = Ledger::fold(evs.iter().enumerate().map(|(i, e)| (i as u64 + 1, e)));
        assert_eq!(l.get(&b).unwrap().status, Status::Decided);
        let open = l.open();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].id, a);
        let h = Health::of(&l);
        assert_eq!(h.presented, 1);
        assert_eq!(h.acted, 1);
        assert_eq!(h.median_seconds_to_decide, Some(40));
    }

    #[test]
    fn one_way_ranks_first_and_reopen_restores_grounded() {
        let a = ItemId::mint("s", "a?");
        let b = ItemId::mint("s", "b?");
        let mut l = Ledger::default();
        l.apply(1, &env(raised(&a, "a?", Door::TwoWay), "s"));
        l.apply(2, &env(raised(&b, "b?", Door::OneWay), "s"));
        assert_eq!(l.open()[0].id, b);
        l.apply(
            3,
            &env(
                LedgerEvent::ItemDefaulted(Defaulted {
                    item_id: a.clone(),
                    option: "A".into(),
                    cause: DefaultCause::TwoWay,
                    reason: "agent's call".into(),
                }),
                "s",
            ),
        );
        assert_eq!(l.unacknowledged_defaults().len(), 1);
        l.apply(
            4,
            &env(
                LedgerEvent::ItemReopened(Reopened {
                    item_id: a.clone(),
                    note: "wrong call".into(),
                    reason: "reopened from digest".into(),
                }),
                "s",
            ),
        );
        assert_eq!(l.get(&a).unwrap().status, Status::Raised);
        assert!(l.unacknowledged_defaults().is_empty());
    }

    #[test]
    fn orphans_are_kept_and_duplicates_found() {
        let a = ItemId::mint("s", "Use Postgres?");
        let mut l = Ledger::default();
        l.apply(
            1,
            &env(
                LedgerEvent::ItemAcknowledged(Acknowledged {
                    item_id: a.clone(),
                    reason: "x".into(),
                }),
                "s",
            ),
        );
        assert_eq!(l.orphans.len(), 1);
        l.apply(2, &env(raised(&a, "Use Postgres?", Door::Unsure), "s"));
        assert!(l.open_with_question("use postgres").is_some());
        assert_eq!(l.resolve(&a.as_str()[3..7]).unwrap().id, a);
    }
}

#[cfg(test)]
mod collision_tests {
    use super::*;
    use crate::event::*;

    #[test]
    fn an_exact_id_wins_over_its_extended_sibling() {
        let short = ItemId::parse("wi-dc4d29").unwrap();
        let long = ItemId::parse("wi-dc4d29f0").unwrap();
        let mut l = Ledger::default();
        for (p, id, q) in [(1, &short, "first?"), (2, &long, "second?")] {
            l.apply(
                p,
                &EventEnvelope {
                    event: LedgerEvent::ItemRaised(Raised {
                        item_id: id.clone(),
                        kind: Kind::Question,
                        question: q.into(),
                        did: None,
                        flip: None,
                        at: None,
                        sym: None,
                        blocks: None,
                        door_hint: Door::Unsure,
                        authority_hint: Authority::Authority,
                        channel: Channel::Marker,
                        quote: q.into(),
                        source_path: None,
                        source_line: None,
                    }),
                    actor: "t".into(),
                    session_id: None,
                    worktree: None,
                    branch: None,
                    head: None,
                    ts: "2026-09-27T00:00:00Z".into(),
                },
            );
        }
        assert_eq!(l.resolve("wi-dc4d29").unwrap().id, short);
        assert_eq!(l.resolve("dc4d29f").unwrap().id, long);
    }
}

//! The ledger's event vocabulary.
//!
//! Every state change of an item is one event, and every event after `item_raised` carries a
//! reason (Seeds: "every state transition takes a reason"). Event type names are snake_case to
//! match redkiln-rs's event-kind table; tags use redkiln's `key:value` shape (`item:`, `session:`,
//! `worktree:`, ...), so a later import into redkiln-rs's store is a tag remap, not a rewrite.

use crate::card::Card;
use crate::id::ItemId;
use crate::marker::{Authority, Door, Kind};
use serde::{Deserialize, Serialize};

/// Where a raised item came from, in falling order of precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Channel {
    /// A `weigh-in` block the agent emitted.
    #[default]
    Marker,
    /// A bullet under an "Open questions" heading, or a new TODO, in a written file.
    Scan,
    /// Found by the in-sitting sweep of the current conversation.
    Sweep,
    /// Raised by hand with `weighin raise`.
    Manual,
}

impl Channel {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Marker => "marker",
            Self::Scan => "scan",
            Self::Sweep => "sweep",
            Self::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Blast {
    File,
    Module,
    System,
    External,
}

impl Blast {
    #[must_use]
    pub fn parse(v: &str) -> Option<Self> {
        match v.trim().to_ascii_lowercase().as_str() {
            "file" => Some(Self::File),
            "module" => Some(Self::Module),
            "system" => Some(Self::System),
            "external" => Some(Self::External),
            _ => None,
        }
    }
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Module => "module",
            Self::System => "system",
            Self::External => "external",
        }
    }
}

/// How the triage pass routed an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Route {
    /// Needs Ryan: authority-bound, one-way, or an unstable recommendation.
    Present,
    /// Judgment-bound two-way door with a stable pick: decided by the agent and logged.
    Default,
    /// Not a question after all (settled, recoverable, duplicate, moot).
    Dismiss,
}

/// The decide-now test, recorded so a deferral can be audited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Timing {
    /// `decide-now` or `defer`.
    pub verdict: String,
    /// The specific fact that would flip the answer. None named means decide now.
    #[serde(default)]
    pub flip: String,
    #[serde(default)]
    pub obtainable_now: bool,
    #[serde(default)]
    pub arrives_after_expiry: bool,
    #[serde(default)]
    pub cost_of_waiting: String,
    #[serde(default)]
    pub options_close: bool,
}

/// Why an item needed no human.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "ref")]
pub enum DismissReason {
    NotUseful,
    Recoverable,
    AlreadyDecided(String),
    DuplicateOf(String),
    NotMyCall(String),
    Moot,
}

impl DismissReason {
    /// Parse `not-useful`, `recoverable`, `moot`, `already-decided:<ref>`, `duplicate-of:<id>`,
    /// `not-my-call:<who>`.
    #[must_use]
    pub fn parse(v: &str) -> Option<Self> {
        let (k, r) = v.split_once(':').map_or((v, ""), |(k, r)| (k, r));
        let r = r.trim().to_owned();
        match k.trim() {
            "not-useful" => Some(Self::NotUseful),
            "recoverable" => Some(Self::Recoverable),
            "moot" => Some(Self::Moot),
            "already-decided" if !r.is_empty() => Some(Self::AlreadyDecided(r)),
            "duplicate-of" | "duplicate" if !r.is_empty() => Some(Self::DuplicateOf(r)),
            "not-my-call" if !r.is_empty() => Some(Self::NotMyCall(r)),
            _ => None,
        }
    }
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::NotUseful => "not-useful".into(),
            Self::Recoverable => "recoverable".into(),
            Self::Moot => "moot".into(),
            Self::AlreadyDecided(r) => format!("already-decided:{r}"),
            Self::DuplicateOf(r) => format!("duplicate-of:{r}"),
            Self::NotMyCall(r) => format!("not-my-call:{r}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provenance {
    /// Ryan chose the recommended option with the trade-off in view.
    UserApproved,
    /// Ryan chose something other than the recommendation, or stated the answer himself.
    UserDirected,
}

impl Provenance {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserApproved => "user-approved",
            Self::UserDirected => "user-directed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DefaultCause {
    /// Judgment-bound two-way door: the agent's own call, logged.
    TwoWay,
    /// Resolve-by passed with nobody acting.
    Expired,
    /// Taken by an unattended run and reported in the digest.
    Unattended,
}

impl DefaultCause {
    #[must_use]
    pub fn parse(v: &str) -> Option<Self> {
        match v.trim() {
            "two-way" => Some(Self::TwoWay),
            "expired" => Some(Self::Expired),
            "unattended" => Some(Self::Unattended),
            _ => None,
        }
    }
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TwoWay => "two-way",
            Self::Expired => "expired",
            Self::Unattended => "unattended",
        }
    }
}

// ----------------------------------------------------------------------------------------------
// Payloads
// ----------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Raised {
    pub item_id: ItemId,
    pub kind: Kind,
    pub question: String,
    /// What the agent did meanwhile (`blocked` when it took no one-way path).
    #[serde(default)]
    pub did: Option<String>,
    #[serde(default)]
    pub flip: Option<String>,
    /// `path:line` anchor, repo-relative.
    #[serde(default)]
    pub at: Option<String>,
    #[serde(default)]
    pub sym: Option<String>,
    #[serde(default)]
    pub blocks: Option<String>,
    pub door_hint: Door,
    pub authority_hint: Authority,
    pub channel: Channel,
    /// The text exactly as the agent or file had it. Transcripts are deleted after 30 days; this
    /// quote is what keeps the item auditable.
    pub quote: String,
    /// File the item was scanned from, so a decision can annotate it.
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub source_line: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Triaged {
    pub item_id: ItemId,
    pub door: Door,
    pub blast: Blast,
    pub authority: Authority,
    pub route: Route,
    #[serde(default)]
    pub timing: Option<Timing>,
    /// `before-merge:<branch>`, `before-release`, an ISO date, or `blocks:<item>`.
    #[serde(default)]
    pub resolve_by: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grounded {
    pub item_id: ItemId,
    pub card: Card,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presented {
    pub item_id: ItemId,
    /// `chat`, `board` or `local`.
    pub surface: String,
    /// The tree the human saw (redkiln-rs ADR-0105 clause 4: temporary index, untracked excluded).
    #[serde(default)]
    pub tree_hash: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decided {
    pub item_id: ItemId,
    pub option: String,
    pub decider: String,
    pub provenance: Provenance,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub tree_hash: Option<String>,
    #[serde(default)]
    pub matched_recommendation: Option<bool>,
    /// `chat`, `board`, `cli`.
    pub via: String,
    #[serde(default)]
    pub seconds_to_decide: Option<u64>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deferred {
    pub item_id: ItemId,
    pub flip: String,
    pub trigger: String,
    pub resolve_by: String,
    pub default_option: String,
    pub owner: String,
    /// Deferred without a passing decide-now test (counted in metrics).
    #[serde(default)]
    pub unqualified: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Defaulted {
    pub item_id: ItemId,
    pub option: String,
    pub cause: DefaultCause,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dismissed {
    pub item_id: ItemId,
    pub why: DismissReason,
    #[serde(default)]
    pub note: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reopened {
    pub item_id: ItemId,
    #[serde(default)]
    pub note: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Superseded {
    pub item_id: ItemId,
    pub by: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Linked {
    pub item_id: ItemId,
    /// `commit`, `pr`, `task`, `atom`, `raised-again`.
    pub kind: String,
    pub target: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordWritten {
    pub item_id: ItemId,
    pub path: String,
    pub atom_id: String,
    /// `decision` or `open_question`.
    pub atom_kind: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Acknowledged {
    pub item_id: ItemId,
    pub reason: String,
}

/// One ledger event. The serde tag is the event type, so the payload bytes stored under a type
/// always decode to that variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LedgerEvent {
    ItemRaised(Raised),
    ItemTriaged(Triaged),
    ItemGrounded(Grounded),
    ItemPresented(Presented),
    ItemDecided(Decided),
    ItemDeferred(Deferred),
    ItemDefaulted(Defaulted),
    ItemDismissed(Dismissed),
    ItemReopened(Reopened),
    ItemSuperseded(Superseded),
    ItemLinked(Linked),
    RecordWritten(RecordWritten),
    ItemAcknowledged(Acknowledged),
}

/// Every event type, for queries that select them all.
pub const EVENT_TYPES: &[&str] = &[
    "item_raised",
    "item_triaged",
    "item_grounded",
    "item_presented",
    "item_decided",
    "item_deferred",
    "item_defaulted",
    "item_dismissed",
    "item_reopened",
    "item_superseded",
    "item_linked",
    "record_written",
    "item_acknowledged",
];

impl LedgerEvent {
    #[must_use]
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::ItemRaised(_) => "item_raised",
            Self::ItemTriaged(_) => "item_triaged",
            Self::ItemGrounded(_) => "item_grounded",
            Self::ItemPresented(_) => "item_presented",
            Self::ItemDecided(_) => "item_decided",
            Self::ItemDeferred(_) => "item_deferred",
            Self::ItemDefaulted(_) => "item_defaulted",
            Self::ItemDismissed(_) => "item_dismissed",
            Self::ItemReopened(_) => "item_reopened",
            Self::ItemSuperseded(_) => "item_superseded",
            Self::ItemLinked(_) => "item_linked",
            Self::RecordWritten(_) => "record_written",
            Self::ItemAcknowledged(_) => "item_acknowledged",
        }
    }

    #[must_use]
    pub fn item_id(&self) -> &ItemId {
        match self {
            Self::ItemRaised(e) => &e.item_id,
            Self::ItemTriaged(e) => &e.item_id,
            Self::ItemGrounded(e) => &e.item_id,
            Self::ItemPresented(e) => &e.item_id,
            Self::ItemDecided(e) => &e.item_id,
            Self::ItemDeferred(e) => &e.item_id,
            Self::ItemDefaulted(e) => &e.item_id,
            Self::ItemDismissed(e) => &e.item_id,
            Self::ItemReopened(e) => &e.item_id,
            Self::ItemSuperseded(e) => &e.item_id,
            Self::ItemLinked(e) => &e.item_id,
            Self::RecordWritten(e) => &e.item_id,
            Self::ItemAcknowledged(e) => &e.item_id,
        }
    }

    /// Whether this event closes the item (it no longer needs anyone).
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::ItemDecided(_)
                | Self::ItemDefaulted(_)
                | Self::ItemDismissed(_)
                | Self::ItemSuperseded(_)
        )
    }
}

/// Who wrote an event, where, and when (by the writer's clock; the store's own `recorded_at` and
/// position are kept separately and positions are the only ordering key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    #[serde(flatten)]
    pub event: LedgerEvent,
    /// `agent`, `hook`, `human:<name>` or `cli`.
    pub actor: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub worktree: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub head: Option<String>,
    /// RFC 3339 UTC.
    pub ts: String,
}

impl EventEnvelope {
    /// `key:value` tags for the store. Values are trimmed to the store's tag limits by the caller.
    #[must_use]
    pub fn tags(&self) -> Vec<(String, String)> {
        let mut t = vec![("item".to_owned(), self.event.item_id().to_string())];
        if let Some(s) = &self.session_id {
            t.push(("session".to_owned(), s.clone()));
        }
        if let Some(w) = &self.worktree {
            t.push(("worktree".to_owned(), w.clone()));
        }
        if let LedgerEvent::ItemRaised(r) = &self.event {
            t.push(("channel".to_owned(), r.channel.as_str().to_owned()));
            t.push(("kind".to_owned(), r.kind.as_str().to_owned()));
            t.push(("door".to_owned(), r.door_hint.as_str().to_owned()));
        }
        t
    }

    /// # Errors
    /// Serialisation of a value made of strings and enums does not fail in practice; the error is
    /// surfaced rather than unwrapped.
    pub fn to_json(&self) -> serde_json::Result<Vec<u8>> {
        serde_json::to_vec(self)
    }

    /// # Errors
    /// Returns the decode error for bytes that are not an envelope.
    pub fn from_json(bytes: &[u8]) -> serde_json::Result<Self> {
        serde_json::from_slice(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_with_the_type_tag() {
        let e = EventEnvelope {
            event: LedgerEvent::ItemDismissed(Dismissed {
                item_id: ItemId::parse("wi-3fa9c1").unwrap(),
                why: DismissReason::DuplicateOf("wi-000aaa".into()),
                note: String::new(),
                reason: "same question raised twice".into(),
            }),
            actor: "cli".into(),
            session_id: Some("s1".into()),
            worktree: None,
            branch: None,
            head: None,
            ts: "2026-09-26T00:00:00Z".into(),
        };
        let bytes = e.to_json().unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(text.contains("\"type\":\"item_dismissed\""), "{text}");
        assert_eq!(EventEnvelope::from_json(&bytes).unwrap(), e);
        assert!(EVENT_TYPES.contains(&e.event.event_type()));
    }

    #[test]
    fn dismiss_reasons_parse() {
        assert_eq!(
            DismissReason::parse("not-my-call:legal team"),
            Some(DismissReason::NotMyCall("legal team".into()))
        );
        assert_eq!(DismissReason::parse("already-decided"), None, "needs a ref");
        assert_eq!(DismissReason::parse("moot"), Some(DismissReason::Moot));
    }
}

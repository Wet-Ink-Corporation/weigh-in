//! The exchange format with the Artifact board: the state document Claude pushes to
//! `board/state`, and the answer documents the page writes under `answers/<item id>`.
//!
//! Answers read back are untrusted input (any viewer with edit rights can write them), so
//! [`validate_answer`] checks every field against the ledger before anything is applied, and
//! free text is only ever stored as a note.

use crate::card::{Card, CardOption, Criterion, Evidence, Excerpt, Recommendation};
use crate::marker::Door;
use crate::projection::{Item, Ledger, Status};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Scope {
    pub repo: String,
    pub session: String,
    #[serde(default)]
    pub source: String,
    pub generated_at: String,
    pub board_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Source {
    pub path: String,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct BoardDecision {
    pub option: String,
    pub decided_at: String,
    pub provenance: String,
    pub matched_recommendation: bool,
    pub applied_at: String,
    #[serde(default)]
    pub ledger_note: String,
}

/// One card on the board. Field names are the page's.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct BoardItem {
    pub id: String,
    pub rank: usize,
    pub question: String,
    pub why: String,
    pub door: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub door_note: Option<String>,
    pub blast: String,
    pub authority: String,
    pub resolve_by: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
    pub timing: String,
    pub source: Source,
    pub evidence: Vec<Evidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<Excerpt>,
    pub options: Vec<CardOption>,
    pub criteria: Vec<Criterion>,
    pub recommendation: Recommendation,
    /// `recommended` or `options-first`.
    pub present: String,
    /// The grounding this card came from; the page echoes it so a stale answer is refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_rev: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<BoardDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct BoardDefault {
    pub id: String,
    pub question: String,
    pub taken: String,
    pub alt: String,
    pub reason: String,
    pub cause: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct BoardSettled {
    pub id: String,
    pub question: String,
    pub answer: String,
    pub decided_at: String,
    pub provenance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct BoardState {
    pub schema: u32,
    pub scope: Scope,
    pub items: Vec<BoardItem>,
    #[serde(default)]
    pub decided: Vec<BoardItem>,
    #[serde(default)]
    pub defaults: Vec<BoardDefault>,
    #[serde(default)]
    pub settled: Vec<BoardSettled>,
}

/// Whether the recommendation is pre-selected (earned and not a one-way door) or held back.
#[must_use]
pub fn presentation(item: &Item, card: &Card) -> &'static str {
    if item.door() == Door::OneWay || !card.recommendation_earned() {
        "options-first"
    } else {
        "recommended"
    }
}

fn board_item(item: &Item, rank: usize, card: &Card) -> BoardItem {
    BoardItem {
        id: item.id.to_string(),
        rank,
        question: item.question.clone(),
        why: card.why.clone(),
        door: item.door().as_str().to_owned(),
        door_note: card.door_note.clone(),
        blast: item.blast.map_or("unknown", |b| b.as_str()).to_owned(),
        authority: item.authority().as_str().to_owned(),
        resolve_by: item
            .resolve_by
            .clone()
            .unwrap_or_else(|| "next sitting".into()),
        blocks: item.blocks.iter().cloned().collect(),
        timing: card.timing.clone(),
        source: Source {
            path: item
                .source_path
                .clone()
                .or_else(|| item.at.clone())
                .unwrap_or_else(|| {
                    format!("session {}", item.session_id.as_deref().unwrap_or("?"))
                }),
            quote: item
                .quote
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(240)
                .collect(),
        },
        evidence: card.evidence.clone(),
        excerpt: card.excerpt.clone(),
        options: card.options.clone(),
        criteria: card.criteria.clone(),
        recommendation: card.recommendation.clone(),
        present: presentation(item, card).to_owned(),
        card_rev: item.card_rev,
        decision: None,
    }
}

/// Build the board document from the ledger. Only grounded items make cards; raised-but-ungrounded
/// items are counted in the scope line but need a sitting before they can be shown.
#[must_use]
pub fn export(ledger: &Ledger, scope: Scope, focus_cap: usize, now: &str) -> BoardState {
    let mut items = Vec::new();
    for i in ledger.open() {
        if i.status == Status::Deferred {
            continue;
        }
        if let Some(card) = &i.card {
            items.push(board_item(i, items.len() + 1, card));
            if items.len() >= focus_cap {
                break;
            }
        }
    }
    let mut decided: Vec<&Item> = ledger
        .items
        .values()
        .filter(|i| i.status == Status::Decided && i.card.is_some())
        .collect();
    decided.sort_by_key(|i| std::cmp::Reverse(i.last_position));
    let decided = decided
        .into_iter()
        .take(20)
        .map(|i| {
            let card = i.card.as_ref().expect("filtered");
            let mut b = board_item(i, 0, card);
            if let Some(d) = &i.decision {
                b.decision = Some(BoardDecision {
                    option: d.option.clone(),
                    decided_at: d.ts.clone(),
                    provenance: d.provenance.as_str().to_owned(),
                    matched_recommendation: d.matched_recommendation.unwrap_or(false),
                    applied_at: d.ts.clone(),
                    ledger_note: d.note.clone(),
                });
            }
            b
        })
        .collect();
    let defaults = ledger
        .items
        .values()
        .filter(|i| i.status == Status::Defaulted)
        .map(|i| {
            let d = i.default.as_ref().expect("defaulted items carry a default");
            let alt = i
                .card
                .as_ref()
                .map(|c| {
                    c.options
                        .iter()
                        .filter(|o| o.id != d.option)
                        .map(|o| o.title.clone())
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .unwrap_or_default();
            let taken = i
                .card
                .as_ref()
                .and_then(|c| c.option(&d.option))
                .map_or_else(|| d.option.clone(), |o| o.title.clone());
            BoardDefault {
                id: i.id.to_string(),
                question: i.question.clone(),
                taken,
                alt,
                reason: d.reason.clone(),
                cause: d.cause.as_str().to_owned(),
            }
        })
        .collect();
    let mut s = scope;
    s.generated_at = now.to_owned();
    BoardState {
        schema: 1,
        scope: s,
        items,
        decided,
        defaults,
        settled: Vec::new(),
    }
}

/// An answer document as the page writes it.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Answer {
    pub item_id: String,
    pub action: String,
    #[serde(default)]
    pub option: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub who: Option<String>,
    #[serde(default)]
    pub defer: Option<AnswerDefer>,
    #[serde(default)]
    pub decided_at: Option<String>,
    #[serde(default)]
    pub seconds_to_decide: Option<u64>,
    #[serde(default)]
    pub applied_at: Option<String>,
    /// The card revision the viewer answered.
    #[serde(default)]
    pub card_rev: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct AnswerDefer {
    pub trigger: String,
    pub resolve_by: String,
    pub default: String,
}

/// What an answer turns into, once validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidAnswer {
    Choose {
        option: String,
        note: String,
        seconds: Option<u64>,
    },
    Defer {
        trigger: String,
        resolve_by: String,
        default: String,
        note: String,
    },
    Dismiss {
        reason: String,
        note: String,
    },
    NotMyCall {
        who: String,
        note: String,
    },
    Reopen {
        note: String,
    },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AnswerError {
    #[error("no item {0} in the ledger")]
    UnknownItem(String),
    #[error("{0} is not open (status {1})")]
    NotOpen(String, String),
    #[error("{0} has no option `{1}`")]
    UnknownOption(String, String),
    #[error("option `{1}` of {0} fails a MUST and cannot be chosen")]
    Ineligible(String, String),
    #[error("a deferral needs a trigger, a resolve-by and a default")]
    IncompleteDeferral,
    #[error(
        "dismiss reason `{0}` is not one of not-useful, already-decided, recoverable, duplicate, moot"
    )]
    BadReason(String),
    #[error("unknown action `{0}`")]
    UnknownAction(String),
    #[error("already applied")]
    AlreadyApplied,
    #[error(
        "{0} was re-grounded after this answer was given (card {1:?}, answer {2:?}); ask again"
    )]
    StaleCard(String, Option<u64>, Option<u64>),
}

fn text(v: Option<&String>) -> String {
    v.map(|s| {
        s.chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .take(2000)
            .collect()
    })
    .unwrap_or_default()
}

/// Check an answer against the ledger.
///
/// # Errors
/// Returns why the answer cannot be applied.
pub fn validate_answer(ledger: &Ledger, a: &Answer) -> Result<ValidAnswer, AnswerError> {
    if a.applied_at.is_some() {
        return Err(AnswerError::AlreadyApplied);
    }
    let item = ledger
        .resolve(&a.item_id)
        .ok_or_else(|| AnswerError::UnknownItem(a.item_id.clone()))?;
    let id = item.id.to_string();
    let note = text(a.note.as_ref());
    if a.action == "reopen" {
        return if matches!(
            item.status,
            Status::Defaulted | Status::Dismissed | Status::Decided
        ) {
            Ok(ValidAnswer::Reopen { note })
        } else {
            Err(AnswerError::NotOpen(id, item.status.as_str().into()))
        };
    }
    if !item.status.is_open() {
        return Err(AnswerError::NotOpen(id, item.status.as_str().into()));
    }
    // An option letter means nothing without the card it was read from.
    if matches!(a.action.as_str(), "choose" | "defer")
        && item.card_rev.is_some()
        && a.card_rev != item.card_rev
    {
        return Err(AnswerError::StaleCard(id, item.card_rev, a.card_rev));
    }
    let option_ok = |o: &str| -> Result<(), AnswerError> {
        match item.card.as_ref().and_then(|c| c.option(o)) {
            Some(opt) if opt.fails_must.is_some() => {
                Err(AnswerError::Ineligible(id.clone(), o.into()))
            }
            Some(_) => Ok(()),
            None => Err(AnswerError::UnknownOption(id.clone(), o.into())),
        }
    };
    match a.action.as_str() {
        "choose" => {
            let o = a.option.clone().unwrap_or_default();
            option_ok(&o)?;
            Ok(ValidAnswer::Choose {
                option: o,
                note,
                seconds: a.seconds_to_decide,
            })
        }
        "defer" => {
            let d = a.defer.as_ref().ok_or(AnswerError::IncompleteDeferral)?;
            if d.trigger.trim().is_empty()
                || d.resolve_by.trim().is_empty()
                || d.default.trim().is_empty()
            {
                return Err(AnswerError::IncompleteDeferral);
            }
            option_ok(&d.default)?;
            Ok(ValidAnswer::Defer {
                trigger: text(Some(&d.trigger)),
                resolve_by: text(Some(&d.resolve_by)),
                default: d.default.clone(),
                note,
            })
        }
        "dismiss" => {
            let r = a.reason.clone().unwrap_or_default();
            if !matches!(
                r.as_str(),
                "not-useful" | "already-decided" | "recoverable" | "duplicate" | "moot"
            ) {
                return Err(AnswerError::BadReason(r));
            }
            Ok(ValidAnswer::Dismiss { reason: r, note })
        }
        "not-my-call" => {
            let who = text(a.who.as_ref());
            if who.trim().is_empty() {
                return Err(AnswerError::BadReason(
                    "not-my-call without a decider".into(),
                ));
            }
            Ok(ValidAnswer::NotMyCall { who, note })
        }
        other => Err(AnswerError::UnknownAction(other.into())),
    }
}

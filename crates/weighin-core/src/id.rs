//! Item ids: `wi-` plus lowercase hex of `sha256(session \n normalised question)`.
//!
//! Hash ids instead of sequence numbers because parallel sessions and worktrees mint ids with
//! no coordinator; sequential ids are what collided in beads, beads_rust and Backlog.md. Six hex
//! characters by default; a writer that finds a different question under the same id extends to
//! eight ([`ItemId::mint_with_len`]).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// Default number of hex characters after the `wi-` prefix.
pub const DEFAULT_HEX_LEN: usize = 6;
/// Length used after a collision is detected.
pub const EXTENDED_HEX_LEN: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemId(String);

impl ItemId {
    /// Mint the default-length id for a question raised in a session.
    #[must_use]
    pub fn mint(session: &str, question: &str) -> Self {
        Self::mint_with_len(session, question, DEFAULT_HEX_LEN)
    }

    /// Mint an id with `hex_len` hex characters (clamped to 4..=16).
    #[must_use]
    pub fn mint_with_len(session: &str, question: &str, hex_len: usize) -> Self {
        let digest =
            Sha256::digest(format!("{session}\n{}", normalise_question(question)).as_bytes());
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let n = hex_len.clamp(4, 16);
        Self(format!("wi-{}", &hex[..n]))
    }

    /// Accept an id typed by a person or read from the store. Returns `None` when the shape is wrong.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let v = value.trim();
        let hex = v.strip_prefix("wi-")?;
        if (4..=16).contains(&hex.len())
            && hex
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            Some(Self(v.to_owned()))
        } else {
            None
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Lowercase, collapse whitespace, drop trailing punctuation. Two raises of the same question
/// with different spacing or a missing `?` are one question.
#[must_use]
pub fn normalise_question(question: &str) -> String {
    let collapsed = question
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    collapsed
        .trim_end_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
        .to_owned()
}

/// Hash of the normalised question alone, used to spot the same question raised in two sessions.
#[must_use]
pub fn question_key(question: &str) -> String {
    let digest = Sha256::digest(normalise_question(question).as_bytes());
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_question_same_session_same_id() {
        let a = ItemId::mint("s1", "Should retention attach per record?");
        let b = ItemId::mint("s1", "  should   retention attach per record ");
        assert_eq!(a, b);
        assert_eq!(a.as_str().len(), 9);
    }

    #[test]
    fn different_session_different_id() {
        assert_ne!(ItemId::mint("s1", "q?"), ItemId::mint("s2", "q?"));
    }

    #[test]
    fn extended_ids_share_a_prefix() {
        let short = ItemId::mint("s", "q");
        let long = ItemId::mint_with_len("s", "q", EXTENDED_HEX_LEN);
        assert!(long.as_str().starts_with(short.as_str()));
    }

    #[test]
    fn parse_rejects_bad_shapes() {
        assert!(ItemId::parse("wi-3fa9c1").is_some());
        assert!(ItemId::parse("wi-3FA9C1").is_none());
        assert!(ItemId::parse("wi-xyz123").is_none());
        assert!(ItemId::parse("3fa9c1").is_none());
    }

    #[test]
    fn question_key_ignores_session() {
        assert_eq!(question_key("Use Postgres?"), question_key("use postgres"));
    }
}

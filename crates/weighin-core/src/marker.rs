//! The `weigh-in` fenced block agents emit when they assume, defer, make an unexamined call, or
//! cannot decide:
//!
//! ````text
//! ```weigh-in
//! kind: assumption          # question | assumption | deferral | call
//! door: one-way             # one-way | two-way | unsure
//! authority: authority      # authority | judgment
//! q: Should retention overrides attach per record or per series?
//! did: per-series, matching RetentionPolicy; per-record not built
//! flip: a tenant needs per-record legal hold
//! at: src/Retention/RetentionPolicy.cs:88
//! blocks: none
//! ```
//! ````
//!
//! Parsing is forgiving about case, spacing and trailing comments, and strict about one thing:
//! a block without `q:` is not an item.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    #[default]
    Question,
    Assumption,
    Deferral,
    Call,
}

impl Kind {
    #[must_use]
    pub fn parse(v: &str) -> Self {
        match v.trim().to_ascii_lowercase().as_str() {
            "assumption" | "assumed" => Self::Assumption,
            "deferral" | "deferred" | "defer" | "todo" => Self::Deferral,
            "call" | "decision" => Self::Call,
            _ => Self::Question,
        }
    }
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Question => "question",
            Self::Assumption => "assumption",
            Self::Deferral => "deferral",
            Self::Call => "call",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Door {
    OneWay,
    TwoWay,
    #[default]
    Unsure,
}

impl Door {
    #[must_use]
    pub fn parse(v: &str) -> Self {
        let v = v.trim().to_ascii_lowercase().replace(['_', ' '], "-");
        match v.as_str() {
            "one-way" | "oneway" | "1-way" | "irreversible" => Self::OneWay,
            "two-way" | "twoway" | "2-way" | "reversible" => Self::TwoWay,
            _ => Self::Unsure,
        }
    }
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OneWay => "one-way",
            Self::TwoWay => "two-way",
            Self::Unsure => "unsure",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Authority {
    /// Needs the human's authority: security, money, data retention, migrations, product calls,
    /// or intent only the user knows.
    #[default]
    Authority,
    /// A judgment the agent is entitled to make and log.
    Judgment,
}

impl Authority {
    #[must_use]
    pub fn parse(v: &str) -> Self {
        match v.trim().to_ascii_lowercase().as_str() {
            "judgment" | "judgement" | "agent" => Self::Judgment,
            _ => Self::Authority,
        }
    }
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Authority => "authority",
            Self::Judgment => "judgment",
        }
    }
}

/// One parsed `weigh-in` block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    pub kind: Kind,
    pub door: Door,
    pub authority: Authority,
    pub question: String,
    pub did: Option<String>,
    pub flip: Option<String>,
    pub at: Option<String>,
    pub sym: Option<String>,
    pub blocks: Option<String>,
    /// Keys this parser does not know, kept verbatim.
    pub extra: BTreeMap<String, String>,
    /// The block exactly as written, for the item's verbatim quote.
    pub raw: String,
}

impl Marker {
    /// `did: blocked` means the agent took no one-way path and continued with other work.
    #[must_use]
    pub fn is_blocked(&self) -> bool {
        self.did
            .as_deref()
            .is_some_and(|d| d.trim().eq_ignore_ascii_case("blocked"))
    }
}

/// Parse every `weigh-in` (or `weighin`) fenced block in `text`.
#[must_use]
pub fn parse_markers(text: &str) -> Vec<Marker> {
    let mut out = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        let Some((fence, info)) = opening_fence(trimmed) else {
            continue;
        };
        if !matches!(
            info.to_ascii_lowercase().as_str(),
            "weigh-in" | "weighin" | "weigh_in"
        ) {
            continue;
        }
        let mut body = Vec::new();
        for inner in lines.by_ref() {
            if inner.trim_start().starts_with(&fence)
                && inner
                    .trim()
                    .chars()
                    .all(|c| c == fence.chars().next().unwrap_or('`'))
            {
                break;
            }
            body.push(inner);
        }
        if let Some(m) = parse_body(&body) {
            out.push(m);
        }
    }
    out
}

fn opening_fence(line: &str) -> Option<(String, String)> {
    let ch = line.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let n = line.chars().take_while(|c| *c == ch).count();
    if n < 3 {
        return None;
    }
    let fence: String = std::iter::repeat_n(ch, n).collect();
    let info = line[n..].split_whitespace().next().unwrap_or("").to_owned();
    Some((fence, info))
}

fn strip_comment(v: &str) -> &str {
    // A comment is ` #` preceded by whitespace; a `#` inside a value (C#, #123) is kept.
    match v.find(" #") {
        Some(i) => v[..i].trim_end(),
        None => v.trim_end(),
    }
}

fn parse_body(body: &[&str]) -> Option<Marker> {
    let mut fields: BTreeMap<String, String> = BTreeMap::new();
    let mut last_key: Option<String> = None;
    for raw in body {
        if raw.trim().is_empty() || raw.trim_start().starts_with('#') {
            continue;
        }
        if let Some((k, v)) = raw.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            if !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                && !raw.starts_with(char::is_whitespace)
            {
                fields.insert(key.clone(), strip_comment(v.trim()).to_owned());
                last_key = Some(key);
                continue;
            }
        }
        // Continuation line: append to the previous value.
        if let Some(k) = &last_key
            && let Some(v) = fields.get_mut(k)
        {
            v.push(' ');
            v.push_str(raw.trim());
        }
    }
    let question = fields
        .remove("q")
        .or_else(|| fields.remove("question"))?
        .trim()
        .to_owned();
    if question.is_empty() {
        return None;
    }
    let take = |f: &mut BTreeMap<String, String>, k: &str| {
        f.remove(k)
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("none") && v != "-")
    };
    let kind = fields
        .remove("kind")
        .map(|v| Kind::parse(&v))
        .unwrap_or_default();
    let door = fields
        .remove("door")
        .map(|v| Door::parse(&v))
        .unwrap_or_default();
    let authority = fields
        .remove("authority")
        .map(|v| Authority::parse(&v))
        .unwrap_or_default();
    let did = take(&mut fields, "did");
    let flip = take(&mut fields, "flip");
    let at = take(&mut fields, "at");
    let sym = take(&mut fields, "sym").or_else(|| take(&mut fields, "symbol"));
    let blocks = take(&mut fields, "blocks");
    Some(Marker {
        kind,
        door,
        authority,
        question,
        did,
        flip,
        at,
        sym,
        blocks,
        extra: fields,
        raw: body.join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Done with the migration.\n\n```weigh-in\nkind: assumption          # question | assumption\ndoor: one-way\nauthority: authority\nq: Should retention overrides attach per record or per series?\ndid: per-series, matching RetentionPolicy; per-record not built\nflip: a tenant needs per-record legal hold\nat: src/Retention/RetentionPolicy.cs:88\nblocks: none\n```\n\nAnd a normal code block:\n```rust\nfn main() {}\n```\n";

    #[test]
    fn parses_the_documented_block() {
        let m = parse_markers(SAMPLE);
        assert_eq!(m.len(), 1);
        let m = &m[0];
        assert_eq!(m.kind, Kind::Assumption);
        assert_eq!(m.door, Door::OneWay);
        assert_eq!(m.authority, Authority::Authority);
        assert_eq!(
            m.question,
            "Should retention overrides attach per record or per series?"
        );
        assert_eq!(m.at.as_deref(), Some("src/Retention/RetentionPolicy.cs:88"));
        assert_eq!(m.blocks, None, "`none` reads as absent");
        assert!(m.did.as_deref().unwrap().starts_with("per-series"));
    }

    #[test]
    fn a_block_without_q_is_not_an_item() {
        assert!(parse_markers("```weigh-in\nkind: call\ndid: x\n```").is_empty());
    }

    #[test]
    fn several_blocks_and_tildes() {
        let t = "~~~weighin\nq: first?\n~~~\ntext\n````weigh-in\nq: second?\ndoor: two-way\nauthority: judgment\n````";
        let m = parse_markers(t);
        assert_eq!(m.len(), 2);
        assert_eq!(m[1].door, Door::TwoWay);
        assert_eq!(m[1].authority, Authority::Judgment);
    }

    #[test]
    fn keeps_hash_inside_values_and_unknown_keys() {
        let m =
            &parse_markers("```weigh-in\nq: Port the C# client or wrap it?\nowner: ryan\n```")[0];
        assert_eq!(m.question, "Port the C# client or wrap it?");
        assert_eq!(m.extra.get("owner").map(String::as_str), Some("ryan"));
    }

    #[test]
    fn continuation_lines_join() {
        let m = &parse_markers("```weigh-in\nq: A long question\n  that wraps?\n```")[0];
        assert_eq!(m.question, "A long question that wraps?");
    }

    #[test]
    fn blocked_is_recognised() {
        let m = &parse_markers("```weigh-in\nq: Drop the column?\ndid: blocked\n```")[0];
        assert!(m.is_blocked());
    }
}

/// Words that mark a question as authority-bound whatever the agent classified it as. A hit keeps
/// a "two-way, judgment" marker open for the sitting instead of logging it as a default: the cheap
/// deterministic half of a two-key rule, erring towards asking.
pub const AUTHORITY_WORDS: &[&str] = &[
    "compliance",
    "compliant",
    "regulat",
    "gdpr",
    "hipaa",
    "fedramp",
    "cjis",
    "pii",
    "personal data",
    "privacy",
    "security",
    "secret",
    "credential",
    "encrypt",
    "permission",
    "access control",
    "retention",
    "residency",
    "legal",
    "contract",
    "licen",
    "sla",
    "billing",
    "pricing",
    "payment",
    "invoice",
    "migration",
    "migrate",
    "breaking change",
    "public api",
    "schema change",
    "delete data",
    "data loss",
    "production",
    "audit",
    "tenant isolation",
    "isolation",
    "customer data",
];

/// The first authority word found in `text`, if any.
#[must_use]
pub fn authority_tripwire(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();
    AUTHORITY_WORDS.iter().copied().find(|w| {
        lower
            .match_indices(w)
            .any(|(i, _)| i == 0 || !lower.as_bytes()[i - 1].is_ascii_alphanumeric())
    })
}

#[cfg(test)]
mod tripwire_tests {
    use super::authority_tripwire;

    #[test]
    fn catches_authority_words_at_word_starts() {
        assert_eq!(
            authority_tripwire("What TTL and tenant-isolation strategy?"),
            Some("isolation")
        );
        assert_eq!(
            authority_tripwire("Is compliance a concern?"),
            Some("compliance")
        );
        assert_eq!(authority_tripwire("Name the helper apply or run?"), None);
        assert_eq!(
            authority_tripwire("reproduction steps"),
            None,
            "`production` must start a word"
        );
    }
}

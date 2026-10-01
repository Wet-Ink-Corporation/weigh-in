//! `.kb` atoms rendered from ledger items, valid against Redkiln's `KbFrontmatter`
//! (`src/schema/kb.ts`): id, title, kind, status, authority_tier, summary, depends_on, related,
//! source_paths, last_reviewed, plus decision-only fields. The schema is `.passthrough()`, so the
//! Weigh-In lineage keys survive validation.
//!
//! The body of a decision atom is MADR 4 (context, drivers, considered options, outcome,
//! consequences, confirmation) plus "why this might be wrong". YAML strings are written as JSON
//! strings, which are valid YAML scalars, so no value can break the frontmatter.

use crate::card::Card;
use crate::marker::Door;
use crate::projection::Item;

/// Where an atom is written, relative to the repository root.
#[must_use]
pub fn decision_path(item: &Item) -> String {
    format!(
        ".kb/_intake/decisions/{}-{}.md",
        item.id,
        slug(&item.question)
    )
}

#[must_use]
pub fn open_question_path(item: &Item) -> String {
    format!(
        ".kb/_intake/open_questions/{}-{}.md",
        item.id,
        slug(&item.question)
    )
}

#[must_use]
pub fn decision_atom_id(item: &Item) -> String {
    format!("kb-decision-{}", item.id)
}

#[must_use]
pub fn open_question_atom_id(item: &Item) -> String {
    format!("kb-open-question-{}", item.id)
}

/// Lowercase ASCII slug of at most 48 characters on a word boundary.
#[must_use]
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let out = out.trim_end_matches('-').to_owned();
    if out.len() <= 48 {
        return out;
    }
    let cut = &out[..48];
    cut.rfind('-').map_or(cut, |i| &cut[..i]).to_owned()
}

/// A YAML-safe double-quoted scalar. JSON escaping covers C0 controls, quotes and backslashes;
/// YAML also refuses DEL, the C1 controls, U+FFFE/U+FFFF and treats U+2028/U+2029 as line breaks,
/// so those are escaped too.
fn q(s: &str) -> String {
    let json = serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into());
    json.chars()
        .map(|c| {
            let u = c as u32;
            if (0x7f..=0x9f).contains(&u) || matches!(u, 0x2028 | 0x2029 | 0xfffe | 0xffff) {
                format!("\\u{u:04x}")
            } else {
                c.to_string()
            }
        })
        .collect()
}

fn list(items: &[String]) -> String {
    if items.is_empty() {
        "[]".into()
    } else {
        format!(
            "[{}]",
            items.iter().map(|s| q(s)).collect::<Vec<_>>().join(", ")
        )
    }
}

/// Reversibility for `KbFrontmatter`: one-way doors are `low`, unsure `medium`, two-way `high`.
#[must_use]
pub fn reversibility(door: Door) -> &'static str {
    match door {
        Door::OneWay => "low",
        Door::Unsure => "medium",
        Door::TwoWay => "high",
    }
}

/// Inputs the item alone does not carry.
#[derive(Debug, Clone, Default)]
pub struct RenderContext {
    /// ISO date (YYYY-MM-DD) for `last_reviewed`.
    pub date: String,
    /// Accepted atom ids that were MUSTs on this card.
    pub depends_on: Vec<String>,
    /// Extra source paths (ledger location, PR URL).
    pub source_paths: Vec<String>,
}

/// The Y-statement used as the atom's summary.
#[must_use]
pub fn y_statement(item: &Item) -> String {
    let Some(dec) = &item.decision else {
        return item.question.clone();
    };
    let card = item.card.as_ref();
    let chosen = card.and_then(|c| c.option(&dec.option));
    let chosen_title = chosen.map_or_else(|| dec.option.clone(), |o| o.title.clone());
    let neglected: Vec<String> = card
        .map(|c| {
            c.options
                .iter()
                .filter(|o| o.id != dec.option)
                .map(|o| o.title.clone())
                .collect()
        })
        .unwrap_or_default();
    let why = card
        .map(|c| c.why.as_str())
        .filter(|w| !w.is_empty())
        .unwrap_or("the question an agent raised");
    let mut s = format!(
        "In the context of \"{}\", facing {}, we decided for {}",
        item.question,
        lower_first(why.trim_end_matches('.')),
        chosen_title
    );
    if !neglected.is_empty() {
        s.push_str(&format!(" and neglected {}", neglected.join("; ")));
    }
    if let Some(o) = chosen {
        if !o.holds_if.is_empty() {
            s.push_str(&format!(", on the premise that {}", o.holds_if));
        }
        if !o.if_wrong.is_empty() {
            s.push_str(&format!(", accepting that if wrong: {}", o.if_wrong));
        }
    }
    s.push('.');
    s
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f)
            if !s.starts_with("I ")
                && !s.chars().nth(1).is_some_and(|x| x.is_ascii_uppercase()) =>
        {
            f.to_lowercase().collect::<String>() + c.as_str()
        }
        Some(f) => f.to_string() + c.as_str(),
        None => String::new(),
    }
}

/// Render the decision atom for a decided item.
#[must_use]
pub fn render_decision(item: &Item, ctx: &RenderContext) -> Option<String> {
    let dec = item.decision.as_ref()?;
    let card = item.card.as_ref();
    let chosen = card.and_then(|c| c.option(&dec.option));
    let title = match chosen {
        Some(o) => o.title.clone(),
        None => format!("{}: {}", item.question.trim_end_matches('?'), dec.option),
    };
    let mut sources = Vec::new();
    if let Some(p) = &item.source_path {
        sources.push(p.clone());
    }
    sources.extend(ctx.source_paths.iter().cloned());
    let related: Vec<String> = item
        .records
        .iter()
        .filter(|r| r.atom_kind == "open_question")
        .map(|r| r.atom_id.clone())
        .collect();

    let mut fm = String::from("---\n");
    fm.push_str(&format!("id: {}\n", q(&decision_atom_id(item))));
    fm.push_str(&format!("title: {}\n", q(&title)));
    fm.push_str("kind: decision\nstatus: accepted\nauthority_tier: decision\n");
    fm.push_str(&format!("summary: {}\n", q(&y_statement(item))));
    fm.push_str(&format!("depends_on: {}\n", list(&ctx.depends_on)));
    fm.push_str(&format!("related: {}\n", list(&related)));
    fm.push_str(&format!("source_paths: {}\n", list(&sources)));
    fm.push_str(&format!("last_reviewed: {}\n", q(&ctx.date)));
    fm.push_str(&format!("reversibility: {}\n", reversibility(item.door())));
    fm.push_str("phase: null\nsupersedes: []\nsuperseded_by: null\n");
    fm.push_str(&format!("weighin_item: {}\n", q(item.id.as_str())));
    fm.push_str(&format!("question: {}\n", q(&item.question)));
    fm.push_str(&format!("door: {}\n", item.door().as_str()));
    if let Some(b) = item.blast {
        fm.push_str(&format!("blast_radius: {}\n", b.as_str()));
    }
    fm.push_str(&format!("decider: {}\n", q(&dec.decider)));
    fm.push_str(&format!("provenance: {}\n", dec.provenance.as_str()));
    fm.push_str(&format!("decided_at: {}\n", q(&dec.ts)));
    if let Some(t) = &dec.tree_hash {
        fm.push_str(&format!("tree_hash: {}\n", q(t)));
    }
    if let Some(r) = card.map(|c| &c.recommendation) {
        fm.push_str(&format!("recommended: {}\n", q(&r.option)));
        if !r.flip.is_empty() {
            fm.push_str(&format!("flip_condition: {}\n", q(&r.flip)));
        }
    }
    fm.push_str("---\n\n");

    let mut b = String::new();
    b.push_str(&format!("# {title}\n\n"));
    b.push_str("## Context and problem statement\n\n");
    b.push_str(&format!("{}\n\n", item.question));
    if let Some(c) = card
        && !c.why.is_empty()
    {
        b.push_str(&format!("{}\n\n", c.why));
    }
    b.push_str(&format!(
        "Raised by an agent ({}) as a {} and captured by Weigh-In as `{}`.",
        item.channel.as_str(),
        item.kind.as_str(),
        item.id
    ));
    if let Some(at) = &item.at {
        b.push_str(&format!(" Anchor: `{at}`."));
    }
    b.push_str("\n\n");
    if let Some(c) = card {
        write_card_sections(&mut b, c, &dec.option);
    }
    b.push_str("## Decision outcome\n\n");
    b.push_str(&format!(
        "Chosen option: **{}**",
        chosen.map_or(dec.option.as_str(), |o| o.title.as_str())
    ));
    match dec.matched_recommendation {
        Some(true) => b.push_str(", the recommended option."),
        Some(false) => b.push_str(&format!(
            ", overriding the recommendation ({}).",
            card.map_or("?", |c| c.recommendation.option.as_str())
        )),
        None => b.push('.'),
    }
    b.push_str("\n\n");
    if !dec.note.is_empty() {
        b.push_str(&format!("Decider's note: {}\n\n", dec.note));
    }
    if let Some(o) = chosen {
        b.push_str("### Consequences\n\n");
        if !o.holds_if.is_empty() {
            b.push_str(&format!(
                "- Good, because it holds if {}.\n",
                o.holds_if.trim_end_matches('.')
            ));
        }
        if !o.if_wrong.is_empty() {
            b.push_str(&format!(
                "- Bad, because if wrong: {}.\n",
                o.if_wrong.trim_end_matches('.')
            ));
        }
        b.push('\n');
    }
    if let Some(c) = card {
        b.push_str("### Confirmation\n\n");
        if c.recommendation.flip.is_empty() {
            b.push_str("Revisit if the premises above stop holding.\n\n");
        } else {
            b.push_str(&format!("Revisit when: {}\n\n", c.recommendation.flip));
        }
        if !c.recommendation.why_wrong.is_empty() {
            b.push_str(&format!(
                "## Why this might be wrong\n\n{}\n\n",
                c.recommendation.why_wrong
            ));
        }
    }
    b.push_str("## Provenance\n\n");
    b.push_str(&format!(
        "- Decided {} by {} ({}), via {}.\n",
        dec.ts,
        dec.decider,
        dec.provenance.as_str(),
        dec.via
    ));
    if let Some(s) = &item.session_id {
        b.push_str(&format!("- Raised in session `{s}`.\n"));
    }
    b.push_str(&format!(
        "- Verbatim: {}\n",
        q(item.quote.lines().next().unwrap_or(""))
    ));
    Some(fm + &b)
}

fn write_card_sections(b: &mut String, c: &Card, chosen: &str) {
    if !c.criteria.is_empty() {
        b.push_str("## Decision drivers\n\n");
        for cr in &c.criteria {
            b.push_str(&format!("- {}\n", cr.name));
        }
        b.push('\n');
    }
    b.push_str("## Considered options\n\n");
    for o in &c.options {
        let mark = if o.id == chosen { " (chosen)" } else { "" };
        b.push_str(&format!("### {} · {}{}\n\n", o.id, o.title, mark));
        if !o.summary.is_empty() {
            b.push_str(&format!("{}\n\n", o.summary));
        }
        if !o.holds_if.is_empty() {
            b.push_str(&format!(
                "- Holds if {}.\n",
                o.holds_if.trim_end_matches('.')
            ));
        }
        if !o.if_wrong.is_empty() {
            b.push_str(&format!(
                "- If wrong: {}.\n",
                o.if_wrong.trim_end_matches('.')
            ));
        }
        if let Some(m) = &o.fails_must {
            b.push_str(&format!("- Fails a MUST: {m}.\n"));
        }
        b.push('\n');
    }
    if !c.evidence.is_empty() {
        b.push_str("## Evidence\n\n");
        for e in &c.evidence {
            b.push_str(&format!("- `{}`: {}\n", e.src, e.text));
        }
        b.push('\n');
    }
}

/// Render the tracking atom for a one-way item deferred past its session.
#[must_use]
pub fn render_open_question(item: &Item, ctx: &RenderContext) -> Option<String> {
    let d = item.deferral.as_ref()?;
    let mut fm = String::from("---\n");
    fm.push_str(&format!("id: {}\n", q(&open_question_atom_id(item))));
    fm.push_str(&format!("title: {}\n", q(&item.question)));
    fm.push_str("kind: open_question\nstatus: proposed\nauthority_tier: note\n");
    fm.push_str(&format!(
        "summary: {}\n",
        q(&format!(
            "Open until {}: {}. Default if unresolved: {}.",
            d.trigger, item.question, d.default_option
        ))
    ));
    fm.push_str(&format!("depends_on: {}\n", list(&ctx.depends_on)));
    fm.push_str("related: []\n");
    fm.push_str(&format!(
        "source_paths: {}\n",
        list(
            &item
                .source_path
                .iter()
                .cloned()
                .chain(ctx.source_paths.iter().cloned())
                .collect::<Vec<_>>()
        )
    ));
    fm.push_str(&format!("last_reviewed: {}\n", q(&ctx.date)));
    fm.push_str(&format!("tracks: {}\n", q(item.id.as_str())));
    fm.push_str(&format!("resolve_by: {}\n", q(&d.resolve_by)));
    fm.push_str(&format!("trigger: {}\n", q(&d.trigger)));
    fm.push_str(&format!(
        "default_if_unresolved: {}\n",
        q(&d.default_option)
    ));
    fm.push_str(&format!("owner: {}\n", q(&d.owner)));
    fm.push_str(&format!("flip_condition: {}\n", q(&d.flip)));
    fm.push_str("---\n\n");
    let mut b = format!("# {}\n\n", item.question);
    b.push_str(&format!(
        "Deferred on purpose. Waiting for: {}. Resolve by: {}. If nobody acts, **{}** applies.\n\n",
        d.trigger, d.resolve_by, d.default_option
    ));
    if let Some(c) = &item.card {
        write_card_sections(&mut b, c, "");
    }
    Some(fm + &b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{CardOption, Recommendation};
    use crate::event::*;
    use crate::id::ItemId;
    use crate::marker::{Authority, Kind};
    use crate::projection::Ledger;

    fn decided_item() -> Item {
        let id = ItemId::mint("s", "Where should the ledger live?");
        let env = |event| EventEnvelope {
            event,
            actor: "t".into(),
            session_id: Some("s".into()),
            worktree: None,
            branch: None,
            head: None,
            ts: "2026-09-26T18:00:00Z".into(),
        };
        let mut l = Ledger::default();
        l.apply(
            1,
            &env(LedgerEvent::ItemRaised(Raised {
                item_id: id.clone(),
                kind: Kind::Question,
                question: "Where should the ledger live?".into(),
                did: None,
                flip: None,
                at: Some("src/a.rs:3".into()),
                sym: None,
                blocks: None,
                door_hint: Door::TwoWay,
                authority_hint: Authority::Authority,
                channel: Channel::Marker,
                quote: "q: \"Where\" should the ledger live?".into(),
                source_path: None,
                source_line: None,
            })),
        );
        l.apply(
            2,
            &env(LedgerEvent::ItemGrounded(Grounded {
                item_id: id.clone(),
                card: Card {
                    why: "It fixes the merge story.".into(),
                    options: vec![
                        CardOption {
                            id: "A".into(),
                            title: "Common-dir store".into(),
                            holds_if: "open items can stay local".into(),
                            if_wrong: "a lost clone loses open items".into(),
                            ..Default::default()
                        },
                        CardOption {
                            id: "B".into(),
                            title: "Committed files: with a colon".into(),
                            ..Default::default()
                        },
                    ],
                    recommendation: Recommendation {
                        option: "A".into(),
                        flip: "others need open items".into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                reason: "grounded".into(),
            })),
        );
        l.apply(
            3,
            &env(LedgerEvent::ItemDecided(Decided {
                item_id: id.clone(),
                option: "A".into(),
                decider: "human:ryan".into(),
                provenance: Provenance::UserApproved,
                note: String::new(),
                tree_hash: Some("abc".into()),
                matched_recommendation: Some(true),
                via: "board".into(),
                seconds_to_decide: Some(29),
                reason: "chose".into(),
            })),
        );
        l.get(&id).unwrap().clone()
    }

    #[test]
    fn decision_atom_has_every_required_field() {
        let item = decided_item();
        let text = render_decision(
            &item,
            &RenderContext {
                date: "2026-09-26".into(),
                ..Default::default()
            },
        )
        .unwrap();
        for f in [
            "id:",
            "title:",
            "kind: decision",
            "status: accepted",
            "authority_tier:",
            "summary:",
            "depends_on:",
            "related:",
            "source_paths:",
            "last_reviewed:",
            "reversibility: high",
        ] {
            assert!(text.contains(f), "missing {f}\n{text}");
        }
        assert!(text.contains("weighin_item: \"wi-"));
        assert!(text.contains("### A · Common-dir store (chosen)"));
        assert!(text.contains("Revisit when: others need open items"));
        // Quotes inside values are escaped, not breaking the YAML.
        assert!(text.contains("\\\"Where\\\""));
    }

    #[test]
    fn yaml_hostile_characters_are_escaped() {
        let v = q("a\u{7f}b\u{85}c\u{2028}d\u{ffff}");
        assert_eq!(v, "\"a\\u007fb\\u0085c\\u2028d\\uffff\"");
    }

    #[test]
    fn slug_is_bounded() {
        assert_eq!(
            slug("Where should the ledger live?"),
            "where-should-the-ledger-live"
        );
        assert!(slug(&"word ".repeat(40)).len() <= 48);
    }
}

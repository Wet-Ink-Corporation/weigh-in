//! Properties of `.kb` atom rendering: whatever the item holds, the front matter keeps its shape
//! and every quoted value reads back as written.

use crate::support::{CASES, Rng, adversarial};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use weighin_core::atom::{RenderContext, render_decision, render_open_question, y_statement};
use weighin_core::card::{Card, CardOption, Criterion, Evidence, Recommendation};
use weighin_core::event::{
    Blast, Channel, Decided, Deferred, EventEnvelope, Grounded, LedgerEvent, Provenance, Raised,
    Route, Triaged,
};
use weighin_core::marker::{Authority, Door, Kind};
use weighin_core::{Item, ItemId, Ledger};

/// Keys every rendered atom carries (Redkiln's `KbFrontmatter`).
const REQUIRED_KEYS: &[&str] = &[
    "id",
    "title",
    "kind",
    "status",
    "authority_tier",
    "summary",
    "depends_on",
    "related",
    "source_paths",
    "last_reviewed",
];

fn text(rng: &mut Rng) -> String {
    adversarial(rng, 8)
}

fn texts(rng: &mut Rng) -> Vec<String> {
    (0..rng.below(3)).map(|_| text(rng)).collect()
}

fn option(rng: &mut Rng, id: &str) -> CardOption {
    CardOption {
        id: id.to_owned(),
        title: text(rng),
        summary: text(rng),
        holds_if: text(rng),
        if_wrong: text(rng),
        fails_must: rng.one_in(3).then(|| text(rng)),
        evasion: None,
    }
}

fn card(rng: &mut Rng) -> Card {
    Card {
        why: text(rng),
        door_note: None,
        timing: text(rng),
        evidence: (0..rng.below(3))
            .map(|_| Evidence {
                src: text(rng),
                text: text(rng),
            })
            .collect(),
        excerpt: None,
        options: vec![option(rng, "A"), option(rng, "B")],
        criteria: (0..rng.below(3))
            .map(|_| Criterion {
                name: text(rng),
                values: BTreeMap::new(),
            })
            .collect(),
        recommendation: Recommendation {
            option: "A".to_owned(),
            flip: text(rng),
            tradeoff: text(rng),
            why_wrong: text(rng),
            stability: None,
        },
        evasion: None,
    }
}

fn envelope(event: LedgerEvent, session: Option<String>, ts: String) -> EventEnvelope {
    EventEnvelope {
        event,
        actor: "test".to_owned(),
        session_id: session,
        worktree: None,
        branch: None,
        head: None,
        ts,
    }
}

fn raised(rng: &mut Rng, id: ItemId, question: String) -> LedgerEvent {
    LedgerEvent::ItemRaised(Raised {
        item_id: id,
        kind: *rng.pick(&[Kind::Question, Kind::Assumption, Kind::Deferral, Kind::Call]),
        quote: text(rng),
        question,
        did: rng.one_in(2).then(|| text(rng)),
        flip: None,
        at: rng.one_in(2).then(|| text(rng)),
        sym: None,
        blocks: None,
        door_hint: *rng.pick(&[Door::OneWay, Door::TwoWay, Door::Unsure]),
        authority_hint: Authority::Authority,
        channel: Channel::Marker,
        source_path: rng.one_in(2).then(|| text(rng)),
        source_line: None,
    })
}

/// A decision, a deferral, or nothing (the item stays open and renders no atom).
fn outcome(rng: &mut Rng, id: ItemId) -> Option<LedgerEvent> {
    match rng.below(3) {
        0 => Some(LedgerEvent::ItemDecided(Decided {
            item_id: id,
            option: (*rng.pick(&["A", "B", "Z"])).to_owned(),
            decider: text(rng),
            provenance: Provenance::UserDirected,
            note: text(rng),
            tree_hash: rng.one_in(2).then(|| text(rng)),
            matched_recommendation: *rng.pick(&[None, Some(true), Some(false)]),
            via: text(rng),
            seconds_to_decide: None,
            reason: text(rng),
        })),
        1 => Some(LedgerEvent::ItemDeferred(Deferred {
            item_id: id,
            flip: text(rng),
            trigger: text(rng),
            resolve_by: text(rng),
            default_option: text(rng),
            owner: text(rng),
            unqualified: false,
            reason: text(rng),
        })),
        _ => None,
    }
}

/// A raised item, folded: triaged, grounded and then decided or deferred, each at random.
fn item(rng: &mut Rng) -> Item {
    let question = text(rng);
    let id = ItemId::mint("s", &question);
    let mut events = vec![raised(rng, id.clone(), question)];
    if rng.one_in(2) {
        events.push(LedgerEvent::ItemTriaged(Triaged {
            item_id: id.clone(),
            door: *rng.pick(&[Door::OneWay, Door::TwoWay, Door::Unsure]),
            blast: *rng.pick(&[Blast::File, Blast::System]),
            authority: Authority::Authority,
            route: Route::Present,
            timing: None,
            resolve_by: None,
            reason: text(rng),
        }));
    }
    if rng.one_in(2) {
        events.push(LedgerEvent::ItemGrounded(Grounded {
            item_id: id.clone(),
            card: card(rng),
            reason: text(rng),
        }));
    }
    events.extend(outcome(rng, id));
    let session = rng.one_in(2).then(|| text(rng));
    let envelopes: Vec<EventEnvelope> = events
        .into_iter()
        .map(|e| envelope(e, session.clone(), text(rng)))
        .collect();
    let ledger = Ledger::fold((1u64..).zip(&envelopes));
    ledger
        .items
        .into_values()
        .next()
        .expect("the raise was folded into an item")
}

fn context(rng: &mut Rng) -> RenderContext {
    RenderContext {
        date: text(rng),
        depends_on: texts(rng),
        source_paths: texts(rng),
    }
}

/// The front matter's `key: value` pairs, checking its shape on the way: it opens on the first
/// line, closes on a `---` line, and each line is one key with a bare token or a JSON value that
/// carries no character YAML refuses or reads as a line break.
fn front_matter(atom: &str, case: u64) -> BTreeMap<String, Value> {
    let rest = atom
        .strip_prefix("---\n")
        .expect("an atom opens with front matter");
    let (head, _body) = rest.split_once("\n---\n").expect("the front matter closes");
    let mut fields = BTreeMap::new();
    for line in head.split('\n') {
        assert!(
            !line.chars().any(|c| c.is_control()
                || matches!(c, '\u{2028}' | '\u{2029}' | '\u{fffe}' | '\u{ffff}')),
            "case {case}: raw control or line break in {line:?}"
        );
        let (key, value) = line.split_once(": ").expect("each line is `key: value`");
        assert!(
            !key.is_empty() && key.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "case {case}: key {key:?}"
        );
        let bare = !value.is_empty()
            && value
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '-' || c == '_');
        let parsed = if bare {
            Value::String(value.to_owned())
        } else {
            serde_json::from_str(value).expect("a quoted value is a JSON scalar or list")
        };
        assert!(
            fields.insert(key.to_owned(), parsed).is_none(),
            "case {case}: {key} repeats"
        );
    }
    for key in REQUIRED_KEYS {
        assert!(fields.contains_key(*key), "case {case}: {key} is missing");
    }
    fields
}

fn string(fields: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    fields.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn strings(fields: &BTreeMap<String, Value>, key: &str) -> Option<Vec<String>> {
    Vec::<String>::deserialize(fields.get(key)?).ok()
}

#[test]
fn decision_atoms_keep_their_front_matter_whatever_the_item_holds() {
    for case in 0..CASES {
        let mut rng = Rng::new(0xa7_0001, case);
        let item = item(&mut rng);
        let ctx = context(&mut rng);

        let Some(atom) = render_decision(&item, &ctx) else {
            assert!(item.decision.is_none(), "case {case}: decided items render");
            continue;
        };

        let fields = front_matter(&atom, case);
        let decision = item.decision.as_ref().expect("only decided items render");
        let summary = y_statement(&item);
        let quoted = [
            ("question", item.question.as_str()),
            ("summary", summary.as_str()),
            ("decider", decision.decider.as_str()),
            ("decided_at", decision.ts.as_str()),
            ("weighin_item", item.id.as_str()),
            ("last_reviewed", ctx.date.as_str()),
        ];
        for (key, expected) in quoted {
            assert_eq!(
                string(&fields, key).as_deref(),
                Some(expected),
                "case {case}: {key}"
            );
        }
        let sources: Vec<String> = item
            .source_path
            .iter()
            .chain(&ctx.source_paths)
            .cloned()
            .collect();
        assert_eq!(
            strings(&fields, "depends_on").as_ref(),
            Some(&ctx.depends_on),
            "case {case}: depends_on"
        );
        assert_eq!(
            strings(&fields, "source_paths"),
            Some(sources),
            "case {case}: source_paths"
        );
        assert_eq!(
            render_decision(&item, &ctx).as_ref(),
            Some(&atom),
            "case {case}: deterministic"
        );
    }
}

#[test]
fn open_question_atoms_keep_their_front_matter_whatever_the_item_holds() {
    for case in 0..CASES {
        let mut rng = Rng::new(0xa7_0002, case);
        let item = item(&mut rng);
        let ctx = context(&mut rng);

        let Some(atom) = render_open_question(&item, &ctx) else {
            assert!(
                item.deferral.is_none(),
                "case {case}: deferred items render"
            );
            continue;
        };

        let fields = front_matter(&atom, case);
        let d = item.deferral.as_ref().expect("only deferred items render");
        let quoted = [
            ("title", item.question.as_str()),
            ("tracks", item.id.as_str()),
            ("trigger", d.trigger.as_str()),
            ("resolve_by", d.resolve_by.as_str()),
            ("owner", d.owner.as_str()),
            ("flip_condition", d.flip.as_str()),
            ("default_if_unresolved", d.default_option.as_str()),
            ("last_reviewed", ctx.date.as_str()),
        ];
        for (key, expected) in quoted {
            assert_eq!(
                string(&fields, key).as_deref(),
                Some(expected),
                "case {case}: {key}"
            );
        }
        assert_eq!(
            strings(&fields, "depends_on").as_ref(),
            Some(&ctx.depends_on),
            "case {case}: depends_on"
        );
        assert_eq!(
            render_open_question(&item, &ctx).as_ref(),
            Some(&atom),
            "case {case}: deterministic"
        );
    }
}

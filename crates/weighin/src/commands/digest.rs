//! `digest`: the defaults taken without you, and the untriaged items from other sessions.

use anyhow::Result;
use std::io::Write;
use weighin_core::{Item, Status};

use super::{clip, write_line};
use crate::JsonArgs;
use crate::env::Env;
use crate::ops::read_ledger;

/// `digest --json`: the defaults and the untriaged items.
#[derive(serde::Serialize)]
struct DigestJson<'l> {
    defaults: &'l [&'l Item],
    untriaged_from_other_sessions: &'l [&'l Item],
}

/// The `digest --json` line. It goes through `serde_json::Value`, whose maps are sorted, so
/// the keys at every level, items included, come out in alphabetical order.
fn digest_json(defaults: &[&Item], untriaged: &[&Item]) -> serde_json::Result<String> {
    let doc = DigestJson {
        defaults,
        untriaged_from_other_sessions: untriaged,
    };
    serde_json::to_string(&serde_json::to_value(&doc)?)
}

pub(crate) fn digest(env: &Env, a: &JsonArgs, out: &mut impl Write) -> Result<()> {
    let ledger = read_ledger(env)?;
    let defaults = ledger.unacknowledged_defaults();
    let untriaged: Vec<&Item> = ledger
        .open()
        .into_iter()
        .filter(|i| i.status == Status::Raised && env.session.as_deref() != i.session_id.as_deref())
        .collect();
    if a.json {
        writeln!(out, "{}", digest_json(&defaults, &untriaged)?)?;
        return Ok(());
    }
    writeln!(
        out,
        "{} default(s) taken without you · {} untriaged item(s) from other sessions",
        defaults.len(),
        untriaged.len()
    )?;
    for i in defaults {
        // `unacknowledged_defaults` only returns items with a default.
        let Some(d) = i.default.as_ref() else {
            continue;
        };
        writeln!(
            out,
            "{} [{}] {} → took: {}",
            i.id,
            d.cause.as_str(),
            clip(&i.question, 90),
            clip(&d.option, 80)
        )?;
    }
    for i in untriaged {
        write_line(out, i)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::digest_json;
    use weighin_core::event::{Channel, Raised};
    use weighin_core::marker::{Authority, Door, Kind};
    use weighin_core::{EventEnvelope, ItemId, Ledger, LedgerEvent};

    fn raised_ledger() -> Ledger {
        let id = ItemId::mint("s1", "Where should the ledger live?");
        let mut l = Ledger::default();
        l.apply(
            1,
            &EventEnvelope {
                event: LedgerEvent::ItemRaised(Raised {
                    item_id: id,
                    kind: Kind::Question,
                    question: "Where should the ledger live?".into(),
                    did: None,
                    flip: None,
                    at: None,
                    sym: None,
                    blocks: None,
                    door_hint: Door::TwoWay,
                    authority_hint: Authority::Authority,
                    channel: Channel::Marker,
                    quote: "Where should the ledger live?".into(),
                    source_path: None,
                    source_line: None,
                }),
                actor: "test".into(),
                session_id: Some("s1".into()),
                worktree: None,
                branch: None,
                head: None,
                ts: "2026-09-26T00:00:00Z".into(),
            },
        );
        l
    }

    #[test]
    fn digest_json_prints_every_key_sorted_as_a_json_value_does() {
        let ledger = raised_ledger();
        let untriaged = ledger.open();
        assert_eq!(untriaged.len(), 1);
        let printed = digest_json(&[], &untriaged).unwrap();
        let expected =
            serde_json::json!({ "defaults": [], "untriaged_from_other_sessions": untriaged })
                .to_string();
        assert_eq!(printed, expected);
        assert!(
            printed.starts_with(
                r#"{"defaults":[],"untriaged_from_other_sessions":[{"also_raised_in":[],"at":null,"#
            ),
            "{printed}"
        );
    }
}

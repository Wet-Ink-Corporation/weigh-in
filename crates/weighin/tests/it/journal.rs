//! Committed journals: a cloud session's ledger events, kept in `.kb/_ledger/journal/` so the
//! pilot's numbers outlive the container (D-0014), and pooled with the ledger by `stats`.

use crate::support::{setup, weighin};
use serde_json::{Value, json};
use std::path::Path;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// One presented item dismissed as not useful, raised and settled in `session`.
fn present_and_dismiss(repo: &Path, session: &str, q: &str) -> TestResult {
    let (ok, out) = weighin(repo, session, &["raise", "--q", q], None);
    if !ok {
        return Err(out.into());
    }
    let id = out
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| format!("no id in `{out}`"))?
        .to_owned();
    let batch = json!([
        {"op": "triage", "id": id, "door": "two-way", "blast": "file", "route": "present", "reason": "a card"},
        {"op": "ground", "id": id, "card": {
            "why": "It names a helper.", "timing": "Decide now.",
            "options": [
                {"id": "A", "title": "apply", "holds_if": "it applies", "if_wrong": "a rename"},
                {"id": "B", "title": "run", "holds_if": "it runs", "if_wrong": "a rename"}
            ],
            "recommendation": {"option": "A", "flip": "it runs"}
        }},
        {"op": "present", "id": id, "surface": "chat"},
        {"op": "dismiss", "id": id, "why": "not-useful", "reason": "noise"}
    ]);
    let (ok, out) = weighin(repo, session, &["batch"], Some(&batch.to_string()));
    if ok { Ok(()) } else { Err(out.into()) }
}

fn stats(repo: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let (ok, out) = weighin(repo, "reader", &["stats", "--json"], None);
    if !ok {
        return Err(out.into());
    }
    Ok(serde_json::from_str(out.trim())?)
}

fn journal(repo: &Path, session: &str) -> Result<String, Box<dyn std::error::Error>> {
    let (ok, out) = weighin(repo, session, &["journal"], None);
    if ok { Ok(out) } else { Err(out.into()) }
}

fn commit_journal(repo: &Path, name: &str, text: &str) -> std::io::Result<()> {
    let dir = repo.join(".kb/_ledger/journal");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(name), text)
}

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn a_journal_alone_gives_the_stats_of_the_ledger_it_came_from() -> TestResult {
    let (_cloud_tmp, cloud) = setup();
    present_and_dismiss(&cloud, "cloud", "Name the helper apply or run?")?;
    let expected = stats(&cloud)?;
    let text = journal(&cloud, "cloud")?;
    let (_fresh_tmp, fresh) = setup();

    commit_journal(&fresh, "cloud.jsonl", &text)?;

    assert_eq!(stats(&fresh)?, expected);
    assert_eq!(expected["presented"], 1, "the fixture presents one item");
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn a_ledger_and_its_own_journals_count_each_event_once() -> TestResult {
    let (_tmp, repo) = setup();
    present_and_dismiss(&repo, "s1", "Name the helper apply or run?")?;
    let before = stats(&repo)?;
    let text = journal(&repo, "s1")?;

    commit_journal(&repo, "s1.jsonl", &text)?;
    commit_journal(&repo, "s1-again.jsonl", &text)?;

    assert_eq!(stats(&repo)?, before);
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn a_journal_from_another_session_adds_its_items() -> TestResult {
    let (_cloud_tmp, cloud) = setup();
    present_and_dismiss(&cloud, "cloud", "Name the helper apply or run?")?;
    let (_local_tmp, local) = setup();
    present_and_dismiss(&local, "local", "Keep the cache warm or cold?")?;

    commit_journal(&local, "cloud.jsonl", &journal(&cloud, "cloud")?)?;
    let h = stats(&local)?;

    assert_eq!((&h["presented"], &h["not_useful"]), (&json!(2), &json!(2)));
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn an_undecodable_journal_line_fails_stats_naming_it() -> TestResult {
    let (_cloud_tmp, cloud) = setup();
    present_and_dismiss(&cloud, "cloud", "Name the helper apply or run?")?;
    let (_tmp, repo) = setup();
    let text = format!("{}not an event\n", journal(&cloud, "cloud")?);
    commit_journal(&repo, "cloud.jsonl", &text)?;

    let (ok, out) = weighin(&repo, "reader", &["stats"], None);

    assert!(!ok, "stats passed over a bad line: {out}");
    assert!(
        out.contains("cloud.jsonl") && out.contains("line 6"),
        "names the file and line: {out}"
    );
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn journal_prints_one_event_per_line_in_ledger_order() -> TestResult {
    let (_tmp, repo) = setup();
    present_and_dismiss(&repo, "s1", "Name the helper apply or run?")?;

    let text = journal(&repo, "s1")?;
    let types = text
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).map(|v| v["type"].as_str().map(str::to_owned)))
        .collect::<Result<Vec<_>, _>>()?;

    let expected = [
        "item_raised",
        "item_triaged",
        "item_grounded",
        "item_presented",
        "item_dismissed",
    ];
    assert_eq!(types, expected.map(|t| Some(t.to_owned())));
    Ok(())
}

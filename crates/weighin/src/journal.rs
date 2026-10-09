//! Committed journals: a ledger's events as JSON lines under `.kb/_ledger/journal/`.
//!
//! A cloud session's ledger goes with its container (D-0014), and with it every presentation,
//! dismissal, deferral and default the pilot is judged on (#23); only decisions survive, as
//! atoms. Such a session writes its ledger's events to `<DIR>/<session>.jsonl` with
//! `weighin journal`, the file is committed with the session's work, and `stats` folds every
//! journal with the ledger. An event held twice, in the ledger and a journal or in two journals,
//! counts once.

use anyhow::{Context as _, Result};
use std::collections::HashSet;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use weighin_core::{EventEnvelope, Ledger};

/// Where journals live, relative to the worktree root.
pub(crate) const DIR: &str = ".kb/_ledger/journal";

/// Write `events` as one JSON envelope per line, in the order given.
///
/// # Errors
/// Fails when an event cannot be encoded or `out` cannot be written.
pub(crate) fn write(events: &[(u64, EventEnvelope)], out: &mut impl Write) -> Result<()> {
    for (_, env) in events {
        out.write_all(&env.to_json()?)?;
        out.write_all(b"\n")?;
    }
    Ok(())
}

/// Every event in the journals under `root`: files in name order, lines in file order, blank
/// lines passed over. No journal directory reads as no events.
///
/// # Errors
/// Fails when the directory or a journal cannot be read, or a line is not an event; the error
/// names the file and line.
pub(crate) fn read(root: &Path) -> Result<Vec<EventEnvelope>> {
    let dir = root.join(DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };
    let mut files = entries
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<PathBuf>>>()
        .with_context(|| format!("reading {}", dir.display()))?;
    files.retain(|p| p.extension().is_some_and(|x| x == "jsonl"));
    files.sort();
    let mut events = Vec::new();
    for file in &files {
        let text =
            std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
        for (n, line) in (1_u64..).zip(text.lines()) {
            if line.trim().is_empty() {
                continue;
            }
            let env = EventEnvelope::from_json(line.as_bytes())
                .with_context(|| format!("{} line {n}: not a ledger event", file.display()))?;
            events.push(env);
        }
    }
    Ok(events)
}

/// The ledger folded from the journals' events followed by its own, each event once (the first
/// copy kept). Order is the sources' own, never timestamps: each journal in its file order, then
/// the ledger in position order. Journals come first because they can hold history the ledger
/// lost: a session resumed in a new container keeps its id but starts an empty ledger, its hook
/// appends the new ledger's events after the old ones, and a question raised again mints the
/// same item id. Folding that ledger first would replay the older events after the newer. A
/// journal of the ledger's own store holds a prefix of it, which this order folds unchanged.
///
/// # Errors
/// Fails when an event cannot be encoded for comparison.
pub(crate) fn pool(
    ledger: Vec<(u64, EventEnvelope)>,
    journal: Vec<EventEnvelope>,
) -> Result<Ledger> {
    if journal.is_empty() {
        return Ok(Ledger::fold(ledger.iter().map(|(p, e)| (*p, e))));
    }
    let mut seen = HashSet::new();
    let mut events = Vec::with_capacity(ledger.len().saturating_add(journal.len()));
    for env in journal
        .into_iter()
        .chain(ledger.into_iter().map(|(_, e)| e))
    {
        if seen.insert(env.to_json()?) {
            events.push(env);
        }
    }
    Ok(Ledger::fold((1_u64..).zip(events.iter())))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAISED: &str = r#"{"type":"item_raised","item_id":"wi-b29e54","kind":"question","question":"Keep the cache warm or cold?","door_hint":"unsure","authority_hint":"authority","channel":"manual","quote":"Keep the cache warm or cold?","actor":"cli","session_id":"s","ts":"2026-10-09T03:29:36Z"}"#;
    const PRESENTED: &str = r#"{"type":"item_presented","item_id":"wi-b29e54","surface":"chat","reason":"","actor":"cli","session_id":"s","ts":"2026-10-09T03:29:37Z"}"#;

    fn events() -> Result<Vec<EventEnvelope>> {
        Ok(vec![
            EventEnvelope::from_json(RAISED.as_bytes())?,
            EventEnvelope::from_json(PRESENTED.as_bytes())?,
        ])
    }

    #[test]
    fn an_event_in_the_ledger_and_a_journal_folds_once() -> Result<()> {
        let ledger = (1_u64..).zip(events()?).collect();

        let pooled = pool(ledger, events()?)?;

        let item = pooled
            .resolve("wi-b29e54")
            .ok_or_else(|| anyhow::anyhow!("item missing"))?;
        assert_eq!((pooled.head, item.times_presented), (2, 1));
        Ok(())
    }
}

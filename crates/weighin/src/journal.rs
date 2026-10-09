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

/// The ledger folded from its own events followed by the journals' events, each event once
/// (the first copy kept). Order is the sources' own, never timestamps: the ledger in position
/// order, then each journal in its file order, which is its store's position order. A journal
/// written earlier from the same store holds a prefix of a later one, so either order of the two
/// folds the same.
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
    for env in ledger.into_iter().map(|(_, e)| e).chain(journal) {
        if seen.insert(env.to_json()?) {
            events.push(env);
        }
    }
    Ok(Ledger::fold((1_u64..).zip(events.iter())))
}

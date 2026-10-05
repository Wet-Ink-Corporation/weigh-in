//! Claude Code hook entry point. One command (`weighin hook`) reads the hook's JSON on stdin and
//! dispatches on `hook_event_name`:
//!
//! * `SessionStart` prints the marker convention and the open counts (added to context).
//! * `Stop` / `SubagentStop` parse `weigh-in` blocks from `last_assistant_message` and raise them.
//! * `PostToolUse` (`Write`, `Edit`, `MultiEdit`) scans only the text the tool wrote.
//!
//! These are command hooks: they cost no model tokens. They never fail a session: any error is
//! appended to `<state dir>/hook-errors.log` and the hook exits 0 with no output. The next
//! `SessionStart` counts the lines added since the last one and tells the user, so capture is
//! never off without a word. (The launcher says so itself when it finds no binary.)
//!
//! In Cowork (`CLAUDE_CODE_ENTRYPOINT=remote_cowork`) hooks run in the cloud workspace, whose
//! project directory is a scratch folder deleted with the session, while the user's files and
//! ledger are on their computer. There the hooks write nothing: `SessionStart` prints the
//! convention, `Stop` nudges on one-way markers, and `/weigh-in` sweeps the conversation and
//! records through the computer's shell.

use anyhow::{Context as _, Result};
use serde_json::Value;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use weighin_core::event::Channel;
use weighin_core::marker::{Door, parse_markers};
use weighin_core::scan::{scan_markdown, scan_todos};

use crate::env::{self, Env};
use crate::ops::{RaiseInput, RaiseOutcome, Session, read_ledger, scannable};

/// Run a hook: read its JSON from `input`, dispatch it, and write what it prints to `output`
/// (possibly nothing). Never fails: a failure to read the input or write the output is logged
/// like any other hook error.
pub(crate) fn entry(input: &mut impl Read, output: &mut impl Write) {
    let mut stdin = String::new();
    // Whatever was read before a failure is still dispatched.
    let read = input.read_to_string(&mut stdin);
    let hook = Hook::parse(&stdin);
    if let Err(e) = read {
        hook.log(&anyhow::Error::new(e).context("reading the hook input"));
    }
    let out = hook.run();
    if let Err(e) = output
        .write_all(out.as_bytes())
        .and_then(|()| output.flush())
    {
        hook.log(&anyhow::Error::new(e).context("writing the hook output"));
    }
}

/// One hook invocation: its payload and the environment it names.
struct Hook {
    payload: Value,
    env: Env,
}

impl Hook {
    fn parse(stdin: &str) -> Self {
        let payload: Value = serde_json::from_str(stdin).unwrap_or(Value::Null);
        let cwd = payload.get("cwd").and_then(Value::as_str).map_or_else(
            || std::env::current_dir().unwrap_or_default(),
            PathBuf::from,
        );
        let session = payload
            .get("session_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let env = Env::discover(&cwd, session);
        Self { payload, env }
    }

    fn event(&self) -> &str {
        self.payload
            .get("hook_event_name")
            .and_then(Value::as_str)
            .unwrap_or("")
    }

    fn log(&self, e: &anyhow::Error) {
        log_error(&self.env, self.event(), e);
    }

    /// What to print on stdout (possibly nothing).
    fn run(&self) -> String {
        let (env, payload, event) = (&self.env, &self.payload, self.event());
        if env::cowork() {
            return match event {
                "SessionStart" => format!("{COWORK_PRIME}{CONVENTION}"),
                "Stop" | "SubagentStop" => cowork_nudge(payload),
                _ => String::new(),
            };
        }
        let result = match event {
            "SessionStart" => {
                let source = payload
                    .get("source")
                    .and_then(Value::as_str)
                    .unwrap_or("startup");
                return session_start(env, source);
            }
            "Stop" | "SubagentStop" => capture(env, payload),
            "PostToolUse" => scan(env, payload),
            _ => Ok(String::new()),
        };
        result.unwrap_or_else(|e| {
            self.log(&e);
            String::new()
        })
    }
}

#[expect(
    clippy::let_underscore_must_use,
    let_underscore_drop,
    reason = "last-resort sink: a hook has no other channel to report a failed append (hooks.rs module doc)"
)]
fn log_error(env: &Env, event: &str, e: &anyhow::Error) {
    let Ok(dir) = env.ensure_state_dir() else {
        return;
    };
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("hook-errors.log"))
    {
        let _ = writeln!(f, "{} {event}: {e:#}", env::now());
    }
}

/// `SessionStart`: the prime for Claude's context and, when hooks have failed since the last
/// session, a one-line message for the user. A prime that fails is logged and reported with the
/// rest, and Claude still gets the convention.
fn session_start(env: &Env, source: &str) -> String {
    let context = prime(env, source).unwrap_or_else(|e| {
        log_error(env, "SessionStart", &e);
        CONVENTION.to_owned()
    });
    match new_errors(env) {
        // Plain stdout reaches only Claude; `systemMessage` is shown to the user.
        Some(message) => serde_json::json!({
            "systemMessage": message,
            "hookSpecificOutput": {
                "hookEventName": "SessionStart",
                "additionalContext": context,
            },
        })
        .to_string(),
        None => context,
    }
}

/// How far `hook-errors.log` had been reported, as a byte offset, beside the log.
const ERRORS_SEEN: &str = "hook-errors.seen";

/// The lines added to `hook-errors.log` since the last `SessionStart` reported it, as a message;
/// records the log's length so each line is reported once. Writes nothing when there is no log,
/// and overwrites rather than deletes (B-01).
fn new_errors(env: &Env) -> Option<String> {
    let dir = env.state_dir();
    let log_path = dir.join("hook-errors.log");
    let log = std::fs::read(&log_path).ok()?;
    let seen = std::fs::read_to_string(dir.join(ERRORS_SEEN))
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .filter(|&n| n <= log.len())
        .unwrap_or(0);
    if seen == log.len() {
        return None;
    }
    let marked = match std::fs::write(dir.join(ERRORS_SEEN), log.len().to_string()) {
        Ok(()) => String::new(),
        Err(e) => format!(" Could not mark this report as seen, so it will repeat: {e}."),
    };
    let added = String::from_utf8_lossy(log.get(seen..)?);
    let lines: Vec<&str> = added.lines().filter(|l| !l.trim().is_empty()).collect();
    let last = lines.last()?;
    let last: String = if last.chars().count() > 200 {
        last.chars().take(199).chain(['…']).collect()
    } else {
        (*last).to_owned()
    };
    Some(format!(
        "Weigh-In: {} hook error(s) since the last session, so capture may be off. Last: {last} (full log: {}){marked}",
        lines.len(),
        log_path.display()
    ))
}

/// The convention, the session key and the counts, in at most ~150 tokens.
///
/// # Errors
/// Fails when the ledger cannot be read.
pub(crate) fn prime(env: &Env, source: &str) -> Result<String> {
    // Read-only: a session that raises nothing must not leave a store behind.
    let ledger = read_ledger(env)?;
    let open = ledger.open();
    let here = env
        .session
        .as_deref()
        .map_or(0, |id| ledger.open_in_session(id).len());
    let one_way = open.iter().filter(|i| i.door() == Door::OneWay).count();
    let defaults = ledger.unacknowledged_defaults().len();
    let mut out = String::new();
    writeln!(
        out,
        "Weigh-In ({source}): session {} · {here} open here · {} open in repo ({one_way} one-way) · {defaults} default(s) to review.",
        env.session.as_deref().unwrap_or("?"),
        open.len()
    )
    .context("formatting the prime")?;
    out.push_str(CONVENTION);
    Ok(out)
}

const COWORK_PRIME: &str = "Weigh-In (cowork): the ledger lives in the user's connected folder, not in this workspace; /weigh-in records there through the computer's shell.\n";

/// Cowork: no ledger here to write, so nudge straight from the markers in the last message.
fn cowork_nudge(payload: &Value) -> String {
    let text = payload
        .get("last_assistant_message")
        .and_then(Value::as_str)
        .unwrap_or("");
    let one_way = parse_markers(text)
        .iter()
        .filter(|m| m.door == Door::OneWay)
        .count();
    if one_way == 0 {
        return String::new();
    }
    let msg = format!(
        "Weigh-In: {one_way} one-way question(s) in that message. Run /weigh-in while the context is fresh."
    );
    serde_json::json!({ "systemMessage": msg }).to_string()
}

const CONVENTION: &str = "When you assume, defer, make a call the user did not examine, or cannot decide, end your message with:\n```weigh-in\nkind: question|assumption|deferral|call\ndoor: one-way|two-way|unsure\nauthority: authority|judgment\nq: <the full question?>\ndid: <what you did meanwhile, or blocked>\nflip: <the fact that would change the answer>\nat: <path:line>\n```\nA list of calls for the user (\"Decisions for you\", \"Your calls\") gets one block per call. No TODO/FIXME/\"revisit later\" without a block. If you cannot ask the user, take no one-way path: `did: blocked`, then continue with independent work.\n";

fn capture(env: &Env, payload: &Value) -> Result<String> {
    let text = payload
        .get("last_assistant_message")
        .and_then(Value::as_str)
        .unwrap_or("");
    let markers = parse_markers(text);
    if markers.is_empty() {
        return Ok(String::new());
    }
    let mut s = Session::open(env, "hook")?;
    let one_way_before = one_way_here(&s, env);
    let mut raised = 0;
    for m in &markers {
        let mut input = RaiseInput::from_marker(m, Channel::Marker);
        if let Some(agent) = payload.get("agent_id").and_then(Value::as_str) {
            input.quote = Some(format!("[subagent {agent}]\n{}", m.raw));
        }
        if matches!(s.raise(&input)?, RaiseOutcome::New(_)) {
            raised += 1;
        }
    }
    let one_way_after = one_way_here(&s, env);
    // One user-visible nudge when this session gains a one-way question; never an automatic sitting.
    if raised > 0 && one_way_after > one_way_before {
        let msg = format!(
            "Weigh-In: {one_way_after} one-way question(s) from this session are waiting. Run /weigh-in while the context is fresh."
        );
        return Ok(serde_json::json!({ "systemMessage": msg }).to_string());
    }
    Ok(String::new())
}

fn one_way_here(s: &Session<'_>, env: &Env) -> usize {
    env.session.as_deref().map_or(0, |id| {
        s.ledger
            .open_in_session(id)
            .iter()
            .filter(|i| i.door() == Door::OneWay)
            .count()
    })
}

fn scan(env: &Env, payload: &Value) -> Result<String> {
    let tool = payload
        .get("tool_name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let input = payload.get("tool_input").cloned().unwrap_or(Value::Null);
    let Some(file) = input.get("file_path").and_then(Value::as_str) else {
        return Ok(String::new());
    };
    let written: Vec<String> = match tool {
        "Write" => input
            .get("content")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .into_iter()
            .collect(),
        "Edit" => input
            .get("new_string")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .into_iter()
            .collect(),
        "MultiEdit" => input
            .get("edits")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|e| {
                        e.get("new_string")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    if written.is_empty() {
        return Ok(String::new());
    }
    scan_text(env, file, &written, "hook")?;
    Ok(String::new())
}

/// Scan text written to `file` (or the whole file, for `weighin scan`) and raise what it finds.
/// Returns the outcomes, one per candidate.
///
/// # Errors
/// Fails when the ledger cannot be written.
pub(crate) fn scan_text(
    env: &Env,
    file: &str,
    written: &[String],
    actor: &str,
) -> Result<Vec<RaiseOutcome>> {
    let rel = env.relative(file);
    // Files outside this repository are not ours to raise from or to annotate later.
    if Path::new(&rel).is_absolute() || rel.starts_with("../") || !scannable(env, &rel) {
        return Ok(Vec::new());
    }
    let is_md = Path::new(&rel)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "md" | "mdx" | "markdown"));
    let mut candidates = Vec::new();
    for w in written {
        if is_md && env.config.scan.markdown {
            candidates.extend(scan_markdown(w));
        } else if !is_md && env.config.scan.todos {
            candidates.extend(scan_todos(w));
        }
    }
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    // Find each candidate's real line in the file as it now stands.
    let on_disk = std::fs::read_to_string(env.root().join(&rel)).unwrap_or_default();
    let mut s = Session::open(env, actor)?;
    let mut out = Vec::new();
    for c in candidates {
        let line = on_disk
            .lines()
            .position(|l| l.trim() == c.quote.trim())
            .map(|i| i + 1);
        let input = RaiseInput {
            q: c.question.clone(),
            kind: Some(if c.todo {
                "deferral".into()
            } else if c
                .heading
                .as_deref()
                .is_some_and(|h| h.to_lowercase().contains("assum"))
            {
                "assumption".into()
            } else {
                "question".into()
            }),
            door: Some("unsure".into()),
            authority: None,
            did: None,
            flip: None,
            at: line.map(|l| format!("{rel}:{l}")),
            sym: None,
            blocks: None,
            channel: Some("scan".into()),
            quote: Some(c.quote.clone()),
            source_path: Some(rel.clone()),
            source_line: line,
        };
        out.push(s.raise(&input)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::entry;
    use std::io::{self, Read, Write};

    /// A reader or writer whose every call fails.
    struct Broken;

    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("input gone"))
        }
    }

    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "output gone"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "output gone"))
        }
    }

    fn payload(dir: &std::path::Path, event: &str) -> String {
        serde_json::json!({ "cwd": dir, "hook_event_name": event }).to_string()
    }

    fn error_log(dir: &std::path::Path) -> String {
        std::fs::read_to_string(dir.join(".weighin").join("hook-errors.log")).unwrap_or_default()
    }

    #[test]
    fn a_hook_that_cannot_write_its_output_logs_it() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let input = payload(tmp.path(), "SessionStart");

        entry(&mut input.as_bytes(), &mut Broken);

        let log = error_log(tmp.path());
        assert!(
            log.contains("SessionStart: writing the hook output: output gone"),
            "{log}"
        );
    }

    #[test]
    fn a_hook_whose_input_fails_dispatches_what_it_read_and_logs_the_failure() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let input = payload(tmp.path(), "Notification");
        let mut output = Vec::new();

        entry(&mut input.as_bytes().chain(Broken), &mut output);

        assert!(output.is_empty(), "{}", String::from_utf8_lossy(&output));
        let log = error_log(tmp.path());
        assert!(
            log.contains("Notification: reading the hook input: input gone"),
            "{log}"
        );
    }

    #[test]
    fn a_hook_prints_what_it_dispatched() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let input = payload(tmp.path(), "SessionStart");
        let mut output = Vec::new();

        entry(&mut input.as_bytes(), &mut output);

        let printed = String::from_utf8(output).expect("utf-8 output");
        assert!(printed.contains("```weigh-in"), "{printed}");
        assert_eq!(error_log(tmp.path()), "");
    }
}

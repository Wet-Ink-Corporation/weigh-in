//! End to end: a scratch repository, hooks fed the JSON Claude Code sends, a linked worktree, a
//! sitting done through `batch`, and the board round trip.

use serde_json::{Value, json};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_weighin")
}

fn git(dir: &Path, args: &[&str]) {
    let st = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .unwrap();
    assert!(
        st.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&st.stderr)
    );
}

fn weighin(dir: &Path, session: &str, args: &[&str], stdin: Option<&str>) -> (bool, String) {
    weighin_env(dir, session, args, stdin, &[])
}

fn weighin_env(
    dir: &Path,
    session: &str,
    args: &[&str],
    stdin: Option<&str>,
    vars: &[(&str, &str)],
) -> (bool, String) {
    let mut cmd = Command::new(bin());
    cmd.current_dir(dir)
        .args(["--session", session])
        .args(args)
        .env_remove("WEIGHIN_STORE")
        .env_remove("CLAUDE_SESSION_ID")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("CLAUDE_CODE_ENTRYPOINT")
        .envs(vars.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    if let Some(s) = stdin {
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(s.as_bytes())
            .unwrap();
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().unwrap();
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    (out.status.success(), text)
}

fn hook(dir: &Path, payload: &Value) -> String {
    let (ok, out) = weighin(dir, "unused", &["hook"], Some(&payload.to_string()));
    assert!(ok, "hook failed: {out}");
    out
}

fn setup() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    std::fs::write(
        repo.join("src/retention.rs"),
        "// Retention rules.\n\npub struct RetentionPolicy;\n\nimpl RetentionPolicy {\n    pub fn series_overrides(&self) -> usize {\n        0\n    }\n}\n\npub fn apply() {\n    let p = RetentionPolicy;\n    let _ = p.series_overrides();\n}\n",
    )
    .unwrap();
    std::fs::write(repo.join("docs/plan.md"), "# Plan\n\nWork.\n").unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    (tmp, repo)
}

const MESSAGE: &str = "Migration done.\n\n```weigh-in\nkind: assumption\ndoor: one-way\nauthority: authority\nq: Should retention overrides attach per record or per series?\ndid: per-series, matching RetentionPolicy\nflip: a tenant needs per-record legal hold\nat: src/retention.rs:6\n```\n\n```weigh-in\nkind: call\ndoor: two-way\nauthority: judgment\nq: Name the helper apply or run?\ndid: named it apply\n```\n";

#[test]
fn a_session_from_capture_to_record() {
    let (_tmp, repo) = setup();
    let (ok, out) = weighin(
        &repo,
        "s1",
        &["init", "--class", "own", "--owner", "ryan"],
        None,
    );
    assert!(ok, "{out}");

    // SessionStart primes the convention.
    let prime = hook(
        &repo,
        &json!({"hook_event_name": "SessionStart", "session_id": "s1", "cwd": repo, "source": "startup"}),
    );
    assert!(
        prime.contains("```weigh-in") && prime.contains("session s1"),
        "{prime}"
    );
    assert!(
        prime.len() < 1100,
        "prime is ~150 tokens, got {} chars",
        prime.len()
    );

    // Stop captures both markers; the one-way one nudges once; the two-way judgment call is defaulted.
    let stop = json!({"hook_event_name": "Stop", "session_id": "s1", "cwd": repo, "last_assistant_message": MESSAGE});
    let nudge = hook(&repo, &stop);
    assert!(
        nudge.contains("systemMessage") && nudge.contains("one-way"),
        "{nudge}"
    );
    assert_eq!(
        hook(&repo, &stop),
        "",
        "a repeated Stop raises nothing and does not nudge again"
    );

    // PostToolUse scans only what was written.
    let plan = "# Plan\n\nWork.\n\n## Open questions\n- Should we shard the ledger by tenant?\n";
    std::fs::write(repo.join("docs/plan.md"), plan).unwrap();
    hook(
        &repo,
        &json!({"hook_event_name": "PostToolUse", "session_id": "s1", "cwd": repo, "tool_name": "Write", "tool_input": {"file_path": repo.join("docs/plan.md"), "content": plan}}),
    );

    let (_, open) = weighin(&repo, "s1", &["open"], None);
    assert!(open.contains("2 open in this session"), "{open}");
    assert!(open.contains("1 default(s) to review"), "{open}");

    // A linked worktree sees the same ledger (it lives under the common dir).
    let wt = repo.parent().unwrap().join("wt");
    git(
        &repo,
        &["worktree", "add", "-q", wt.to_str().unwrap(), "-b", "feat"],
    );
    let (_, from_wt) = weighin(&wt, "s2", &["open", "--repo", "--json"], None);
    let items: Vec<Value> = serde_json::from_str(from_wt.trim()).unwrap();
    assert_eq!(items.len(), 2, "{from_wt}");
    let one_way = items.iter().find(|i| i["door_hint"] == "one-way").unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let scanned = items.iter().find(|i| i["channel"] == "scan").unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        !repo.join(".weighin").exists() && !wt.join(".weighin").exists(),
        "nothing is written into the working trees"
    );

    // Context pack: excerpt, symbol, call sites.
    let (_, ctx) = weighin(&repo, "s1", &["context", &one_way], None);
    assert!(
        ctx.contains("series_overrides") && ctx.contains("src/retention.rs:13"),
        "{ctx}"
    );
    assert!(ctx.contains("vscode://file/"), "{ctx}");

    // The sitting: triage, ground and present in one call.
    let card = json!({
        "why": "It fixes what a tenant can hold.",
        "timing": "Decide now.",
        "evidence": [{"src": "src/retention.rs:6", "text": "series_overrides exists; no per-record path"}],
        "options": [
            {"id": "A", "title": "Per series", "holds_if": "no tenant needs per-record holds", "if_wrong": "a migration of every override"},
            {"id": "B", "title": "Per record", "holds_if": "legal hold is per record", "if_wrong": "slower queries"}
        ],
        "recommendation": {"option": "A", "flip": "a tenant needs per-record legal hold", "stability": {"agree": 4, "of": 4}}
    });
    let batch = json!([
        {"op": "triage", "id": one_way, "door": "one-way", "blast": "system", "route": "present", "reason": "authority-bound one-way door"},
        {"op": "ground", "id": one_way, "card": card},
        {"op": "present", "id": one_way, "surface": "chat"},
        {"op": "decide", "id": one_way, "option": "Z"}
    ]);
    let (ok, out) = weighin(&repo, "s1", &["batch"], Some(&batch.to_string()));
    assert!(
        !ok && out.contains("has no option `Z`") && out.contains("✓ presented"),
        "bad option refused, the rest applied: {out}"
    );

    let (ok, out) = weighin(
        &repo,
        "s1",
        &[
            "decide",
            &one_way,
            "A",
            "--note",
            "series is what the schema has",
            "--via",
            "chat",
        ],
        None,
    );
    assert!(ok && out.contains("atom .kb/_intake/decisions/"), "{out}");
    let atom_dir = repo.join(".kb/_intake/decisions");
    let atom = std::fs::read_dir(&atom_dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let text = std::fs::read_to_string(&atom).unwrap();
    assert!(
        text.contains("kind: decision")
            && text.contains("reversibility: low")
            && text.contains("provenance: user-approved"),
        "{text}"
    );
    assert!(
        text.contains("tree_hash: \""),
        "records the tree the human saw"
    );

    // Deciding a scanned item annotates its source line.
    let batch = json!([
        {"op": "ground", "id": scanned, "card": {"options": [{"id": "A", "title": "Shard"}, {"id": "B", "title": "One store"}], "recommendation": {"option": "B"}}},
        {"op": "decide", "id": scanned, "option": "B"}
    ]);
    let (ok, out) = weighin(&repo, "s1", &["batch"], Some(&batch.to_string()));
    assert!(ok, "{out}");
    let plan_after = std::fs::read_to_string(repo.join("docs/plan.md")).unwrap();
    assert!(
        plan_after.contains(&format!("→ Weigh-In {scanned}: decided B")),
        "{plan_after}"
    );

    // Rescanning the annotated file raises nothing new.
    hook(
        &repo,
        &json!({"hook_event_name": "PostToolUse", "session_id": "s1", "cwd": repo, "tool_name": "Write", "tool_input": {"file_path": repo.join("docs/plan.md"), "content": plan_after}}),
    );
    let (_, open) = weighin(&repo, "s1", &["open"], None);
    assert!(open.contains("No decisions need you."), "{open}");

    // Digest shows the default; the board can reopen it.
    let (_, digest) = weighin(&repo, "s1", &["digest", "--json"], None);
    let d: Value = serde_json::from_str(digest.trim()).unwrap();
    let default_id = d["defaults"][0]["id"].as_str().unwrap().to_owned();
    let (ok, out) = weighin(
        &repo,
        "s1",
        &["board", "export", "--out", "board.json"],
        None,
    );
    assert!(
        ok && out.contains("2 decided") && out.contains("1 default(s)"),
        "{out}"
    );
    let answers = repo.join("answers.json");
    std::fs::write(&answers, json!([
        {"id": default_id, "data": {"item_id": default_id, "action": "reopen", "note": "prefer run"}},
        {"id": "wi-000000", "data": {"item_id": "wi-000000", "action": "choose", "option": "A"}}
    ]).to_string()).unwrap();
    let (ok, out) = weighin(
        &repo,
        "s1",
        &["board", "apply", answers.to_str().unwrap()],
        None,
    );
    assert!(
        ok && out.contains(&format!("reopened {default_id}")) && out.contains("✗ wi-000000"),
        "{out}"
    );

    let (_, stats) = weighin(&repo, "s1", &["stats", "--json"], None);
    let h: Value = serde_json::from_str(stats.trim()).unwrap();
    assert_eq!(h["presented"], 1);
    assert_eq!(h["acted"], 1);
}

#[test]
fn hooks_never_fail_a_session() {
    let tmp = tempfile::tempdir().unwrap();
    let out = hook(
        tmp.path(),
        &json!({"hook_event_name": "Stop", "cwd": tmp.path(), "last_assistant_message": "no markers here"}),
    );
    assert_eq!(out, "");
    let (ok, _) = weighin(tmp.path(), "x", &["hook"], Some("not json"));
    assert!(ok);
}

#[test]
fn session_start_reports_hook_errors_since_the_last_session() {
    let (_tmp, repo) = setup();
    let start = json!({"hook_event_name": "SessionStart", "session_id": "s1", "cwd": repo, "source": "startup"});
    let state = repo.join(".git/weighin");
    std::fs::create_dir_all(&state).unwrap();
    let log = state.join("hook-errors.log");
    std::fs::write(
        &log,
        "2026-10-02T09:00:00Z Stop: opening ledger: disk full\n2026-10-02T09:01:00Z PostToolUse: \"quoted\" failure\n",
    )
    .unwrap();

    // Two new lines: one message for the user, the prime still in Claude's context.
    let out: Value = serde_json::from_str(&hook(&repo, &start)).unwrap();
    let message = out["systemMessage"].as_str().unwrap();
    assert!(
        message.contains("2 hook error(s)")
            && message.contains("\"quoted\" failure")
            && message.contains("hook-errors.log"),
        "{message}"
    );
    assert!(!message.contains('\n'), "one line: {message}");
    assert_eq!(out["hookSpecificOutput"]["hookEventName"], "SessionStart");
    assert!(
        out["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains("```weigh-in")
    );

    // Reported once: the next session is the plain prime.
    let again = hook(&repo, &start);
    assert!(
        again.starts_with("Weigh-In (startup)") && !again.contains("systemMessage"),
        "{again}"
    );

    // A line added later is the only one counted.
    let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
    writeln!(f, "2026-10-02T10:00:00Z Stop: locked").unwrap();
    let out: Value = serde_json::from_str(&hook(&repo, &start)).unwrap();
    let message = out["systemMessage"].as_str().unwrap();
    assert!(
        message.contains("1 hook error(s)") && message.contains("Stop: locked"),
        "{message}"
    );

    // A ledger SessionStart cannot read is reported in the same session, and Claude still gets
    // the convention.
    let folder = repo.parent().unwrap().join("broken");
    std::fs::create_dir_all(folder.join(".weighin")).unwrap();
    std::fs::write(folder.join(".weighin/ledger.sqlite3"), b"not a database").unwrap();
    let out: Value = serde_json::from_str(&hook(
        &folder,
        &json!({"hook_event_name": "SessionStart", "session_id": "s1", "cwd": folder}),
    ))
    .unwrap();
    let message = out["systemMessage"].as_str().unwrap();
    assert!(
        message.contains("1 hook error(s)") && message.contains("SessionStart"),
        "{message}"
    );
    assert!(
        out["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains("```weigh-in")
    );
}

#[test]
fn batch_refs_reach_items_raised_in_the_same_call() {
    let (_tmp, repo) = setup();
    let batch = json!([
        {"op": "raise", "q": "Keep the retry at three attempts?", "door": "two-way", "authority": "judgment", "channel": "sweep"},
        {"op": "triage", "id": "@1", "door": "two-way", "blast": "file", "route": "dismiss", "reason": "the config already says three"},
        {"op": "dismiss", "id": "@1", "why": "recoverable", "reason": "appsettings.json sets RetryCount=3"},
        {"op": "dismiss", "id": "@2", "why": "moot", "reason": "no second raise"}
    ]);
    let (ok, out) = weighin(&repo, "s9", &["batch"], Some(&batch.to_string()));
    assert!(!ok, "the bad ref fails the batch exit code");
    assert!(
        out.contains("✓ dismissed")
            && out.contains("recoverable")
            && out.contains("only 1 raise(s) so far"),
        "{out}"
    );
}

#[test]
fn authority_items_are_not_papered_over() {
    let (_tmp, repo) = setup();
    let batch = json!([
        {"op": "raise", "q": "Must per-tenant isolation meet a compliance boundary?", "door": "unsure", "channel": "sweep"},
        {"op": "dismiss", "id": "@1", "why": "already-decided:task-directive", "reason": "user said assume and keep going"},
        {"op": "dismiss", "id": "@1", "why": "already-decided:chat:\"key-level isolation is fine, it's not regulated\"", "reason": "user's words"}
    ]);
    let (ok, out) = weighin(&repo, "s7", &["batch"], Some(&batch.to_string()));
    assert!(!ok, "{out}");
    assert!(
        out.contains("is authority-bound (`compliance`)") && out.contains("✓ dismissed"),
        "{out}"
    );
}

#[test]
fn review_regressions() {
    let (_tmp, repo) = setup();

    // #8: SessionStart in a plain folder leaves nothing behind.
    let plain = repo.parent().unwrap().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    hook(
        &plain,
        &json!({"hook_event_name": "SessionStart", "session_id": "s1", "cwd": plain, "source": "startup"}),
    );
    assert!(
        !plain.join(".weighin").exists(),
        "prime must not create a store"
    );

    // #6: a Write outside the repository is not scanned.
    let outside = repo.parent().unwrap().join("outside.md");
    let text = "## Open questions\n- Should the outside file be touched?\n";
    std::fs::write(&outside, text).unwrap();
    hook(
        &repo,
        &json!({"hook_event_name": "PostToolUse", "session_id": "s1", "cwd": repo, "tool_name": "Write", "tool_input": {"file_path": outside, "content": text}}),
    );
    let (_, open) = weighin(&repo, "s1", &["open", "--repo"], None);
    assert!(open.contains("0 open in repo"), "{open}");

    // #7: annotating a CRLF file keeps it CRLF.
    let spec = "# Spec\r\n\r\n## Open questions\r\n- Keep the queue durable?\r\n";
    std::fs::write(repo.join("docs/spec.md"), spec).unwrap();
    let (_, out) = weighin(&repo, "s1", &["scan", "docs/spec.md"], None);
    let id = out.split_whitespace().nth(1).unwrap().to_owned();
    let (ok, out) = weighin(&repo, "s1", &["decide", &id, "yes"], None);
    assert!(ok, "{out}");
    let after = std::fs::read_to_string(repo.join("docs/spec.md")).unwrap();
    assert!(
        after.contains("decided yes")
            && after.matches("\r\n").count() == 4
            && !after.replace("\r\n", "").contains('\n'),
        "{after:?}"
    );

    // #5: a settled question raised again (another session) is not re-asked.
    let (_, out) = weighin(
        &repo,
        "s2",
        &["raise", "--q", "Keep the queue durable?"],
        None,
    );
    assert!(out.contains(&format!("{id} already raised")), "{out}");

    // #3: a bare defer next to a decide in one batch: both apply.
    let batch = json!([
        {"op": "raise", "q": "Pick A or B for the cache?", "door": "two-way", "channel": "sweep"},
        {"op": "raise", "q": "Which region hosts the backup?", "door": "one-way", "channel": "sweep"},
        {"op": "decide", "id": "@1", "option": "A"},
        {"op": "defer", "id": "@2", "unqualified": true, "trigger": "next sitting", "resolve_by": "2026-10-04", "default": "west"},
        {"op": "no-such-op", "id": "@1"}
    ]);
    let (_, out) = weighin(&repo, "s3", &["batch"], Some(&batch.to_string()));
    assert!(
        out.contains("✓ decided") && out.contains("✓ deferred") && out.contains("✗ op 4"),
        "{out}"
    );

    // #2: an answer to an older card is refused.
    let batch = json!([
        {"op": "raise", "q": "Which store for sessions?", "door": "two-way", "channel": "sweep"},
        {"op": "ground", "id": "@1", "card": {"options": [{"id": "A", "title": "Postgres"}, {"id": "B", "title": "SQLite"}], "recommendation": {"option": "A"}}}
    ]);
    let (ok, out) = weighin(&repo, "s4", &["batch"], Some(&batch.to_string()));
    assert!(ok, "{out}");
    let (_, j) = weighin(&repo, "s4", &["open", "--json"], None);
    let items: Vec<Value> = serde_json::from_str(j.trim()).unwrap();
    let sid = items[0]["id"].as_str().unwrap().to_owned();
    let old_rev = items[0]["card_rev"].as_u64().unwrap();
    let regrounded = json!([{"op": "ground", "id": sid, "card": {"options": [{"id": "A", "title": "SQLite"}, {"id": "B", "title": "Postgres"}], "recommendation": {"option": "A"}}}]);
    weighin(&repo, "s4", &["batch"], Some(&regrounded.to_string()));
    let answers = repo.join("stale.json");
    std::fs::write(
        &answers,
        json!({"item_id": sid, "action": "choose", "option": "B", "card_rev": old_rev}).to_string(),
    )
    .unwrap();
    let (_, out) = weighin(
        &repo,
        "s4",
        &["board", "apply", answers.to_str().unwrap()],
        None,
    );
    assert!(
        out.contains("re-grounded after this answer") && out.contains("applied: none"),
        "{out}"
    );
}

/// Runs `cmd` to completion, killing it and failing the test if it outlives `limit` (a looping
/// launcher). The deadline is kept here rather than with `timeout(1)`, which on Windows can
/// resolve to `TIMEOUT.EXE` from System32, a different program.
fn run_within(cmd: &mut Command, limit: std::time::Duration) -> std::process::Output {
    use std::io::Read as _;
    let deadline = std::time::Instant::now() + limit;
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("sh is on PATH");
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let (tx_out, rx) = std::sync::mpsc::sync_channel::<(bool, Vec<u8>)>(2);
    let tx_err = tx_out.clone();
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .name("launcher-stdout".into())
            .spawn_scoped(s, move || {
                let mut buf = Vec::new();
                stdout.read_to_end(&mut buf).expect("stdout is readable");
                tx_out
                    .send((true, buf))
                    .expect("the test thread is listening");
            })
            .expect("the stdout reader starts");
        std::thread::Builder::new()
            .name("launcher-stderr".into())
            .spawn_scoped(s, move || {
                let mut buf = Vec::new();
                stderr.read_to_end(&mut buf).expect("stderr is readable");
                tx_err
                    .send((false, buf))
                    .expect("the test thread is listening");
            })
            .expect("the stderr reader starts");
        let (mut out, mut err) = (Vec::new(), Vec::new());
        for _ in 0..2 {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            match rx.recv_timeout(left) {
                Ok((true, buf)) => out = buf,
                Ok((false, buf)) => err = buf,
                Err(_) => {
                    child.kill().expect("the looping launcher can be killed");
                    child.wait().expect("the killed launcher is reaped");
                    panic!("the launcher did not finish within {limit:?}");
                }
            }
        }
        let status = child.wait().expect("the launcher exits");
        std::process::Output {
            status,
            stdout: out,
            stderr: err,
        }
    })
}

#[test]
fn launcher_does_not_loop_when_no_binary_exists() {
    let tmp = tempfile::tempdir().unwrap();
    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let launcher = bin_dir.join("weighin");
    std::fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../plugin/libexec/weighin"),
        &launcher,
    )
    .unwrap();
    let parent = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(std::iter::once(bin_dir).chain(std::env::split_paths(&parent)))
        .unwrap();
    // Generous for a slow shared CI runner (a normal run takes under a second); a looping
    // launcher never finishes, so it still fails.
    let limit = std::time::Duration::from_secs(30);
    let start = std::time::Instant::now();
    let out = run_within(
        Command::new("sh")
            .arg(&launcher)
            .arg("open")
            .env("PATH", &path),
        limit,
    );
    assert_eq!(
        out.status.code(),
        Some(127),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(start.elapsed() < limit);
    let hook = run_within(
        Command::new("sh")
            .arg(&launcher)
            .arg("hook")
            .env("PATH", &path),
        limit,
    );
    assert_eq!(
        hook.status.code(),
        Some(0),
        "a missing binary never fails a hook"
    );
    assert!(hook.stdout.is_empty(), "only SessionStart speaks");

    // SessionStart says capture is off, in a message shown to the user, and still exits 0.
    let start = run_within(
        Command::new("sh")
            .arg(&launcher)
            .args(["hook", "SessionStart"])
            .env("PATH", &path)
            .env_remove("CLAUDE_CODE_ENTRYPOINT"),
        limit,
    );
    assert_eq!(start.status.code(), Some(0));
    let out: Value = serde_json::from_slice(&start.stdout).unwrap();
    assert!(
        out["systemMessage"]
            .as_str()
            .unwrap()
            .contains("capture is off"),
        "{out}"
    );

    // Cowork captures through the sitting, not hooks: nothing is said there (B-04).
    let cowork = run_within(
        Command::new("sh")
            .arg(&launcher)
            .args(["hook", "SessionStart"])
            .env("PATH", &path)
            .env("CLAUDE_CODE_ENTRYPOINT", "remote_cowork"),
        limit,
    );
    assert_eq!(cowork.status.code(), Some(0));
    assert!(cowork.stdout.is_empty());
}

/// The target directory the `sh` launcher looks in first on this platform.
fn launcher_target() -> Option<(&'static str, &'static str)> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some(("x86_64-unknown-linux-musl", ""))
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some(("aarch64-apple-darwin", ""))
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some(("x86_64-pc-windows-msvc", ".exe"))
    } else {
        None
    }
}

#[test]
fn launcher_passes_session_start_to_the_binary_without_the_event_name() {
    let Some((target, ext)) = launcher_target() else {
        return;
    };
    let (_tmp, repo) = setup();
    let plugin = repo.parent().unwrap().join("plugin");
    std::fs::create_dir_all(plugin.join(target)).unwrap();
    let launcher = plugin.join("weighin");
    std::fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../plugin/libexec/weighin"),
        &launcher,
    )
    .unwrap();
    std::fs::copy(bin(), plugin.join(target).join(format!("weighin{ext}"))).unwrap();
    let payload = repo.parent().unwrap().join("start.json");
    std::fs::write(
        &payload,
        json!({"hook_event_name": "SessionStart", "session_id": "s1", "cwd": repo, "source": "startup"})
            .to_string(),
    )
    .unwrap();
    let out = run_within(
        Command::new("sh")
            .arg(&launcher)
            .args(["hook", "SessionStart"])
            .env_remove("CLAUDE_CODE_ENTRYPOINT")
            .stdin(std::fs::File::open(&payload).unwrap()),
        std::time::Duration::from_secs(30),
    );
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.starts_with("Weigh-In (startup): session s1"),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn cowork_hooks_write_nothing_and_still_nudge() {
    let (_tmp, repo) = setup();
    let cowork = [("CLAUDE_CODE_ENTRYPOINT", "remote_cowork")];
    let run = |payload: &Value| {
        let (ok, out) = weighin_env(
            &repo,
            "unused",
            &["hook"],
            Some(&payload.to_string()),
            &cowork,
        );
        assert!(ok, "{out}");
        out
    };
    let prime = run(&json!({"hook_event_name": "SessionStart", "session_id": "s1", "cwd": repo}));
    assert!(
        prime.contains("(cowork)") && prime.contains("```weigh-in"),
        "{prime}"
    );
    let nudge = run(
        &json!({"hook_event_name": "Stop", "session_id": "s1", "cwd": repo, "last_assistant_message": MESSAGE}),
    );
    assert!(nudge.contains("1 one-way question"), "{nudge}");
    let edit = run(
        &json!({"hook_event_name": "PostToolUse", "session_id": "s1", "cwd": repo, "tool_name": "Write",
        "tool_input": {"file_path": repo.join("docs/plan.md"), "content": "## Open questions\n- Which tenant first?\n"}}),
    );
    assert!(edit.is_empty(), "{edit}");
    assert!(
        !repo.join(".git/weighin").exists(),
        "no ledger in the Cowork scratch folder"
    );
}

#[test]
fn the_ledger_goes_where_the_folder_says() {
    // A non-git folder, as a Cowork connected folder usually is.
    let tmp = tempfile::tempdir().unwrap();
    let folder = tmp.path().join("Rulings");
    std::fs::create_dir_all(&folder).unwrap();
    let raise = r#"[{"op":"raise","q":"Which rule wins when two rulings conflict?","kind":"question","door":"two-way","authority":"authority","channel":"sweep"}]"#;

    // Default: <folder>/.weighin/, under a TRUNCATE journal.
    let (ok, out) = weighin(&folder, "s1", &["batch"], Some(raise));
    assert!(ok, "{out}");
    let default = folder.join(".weighin/ledger.sqlite3");
    assert!(default.exists());
    assert_eq!(
        std::fs::metadata(folder.join(".weighin/ledger.sqlite3-journal"))
            .unwrap()
            .len(),
        0
    );
    assert!(!folder.join(".weighin/ledger.sqlite3-wal").exists());

    // `init --ledger` moves it outside the folder, relative to the folder root.
    let (ok, out) = weighin(
        &folder,
        "s2",
        &[
            "init",
            "--class",
            "own",
            "--ledger",
            "../ledgers/rulings.sqlite3",
        ],
        None,
    );
    assert!(ok, "{out}");
    assert!(out.contains("ledgers"), "{out}");
    let (ok, out) = weighin(&folder, "s2", &["batch"], Some(raise));
    assert!(ok, "{out}");
    assert!(tmp.path().join("ledgers/rulings.sqlite3").exists());
    let (ok, out) = weighin(&folder, "s2", &["open", "--repo"], None);
    assert!(ok && out.contains("Which rule wins"), "{out}");

    // `--store` wins over the config.
    let flag = tmp.path().join("flag.sqlite3");
    let (ok, out) = weighin(
        &folder,
        "s3",
        &["--store", flag.to_str().unwrap(), "open", "--repo"],
        None,
    );
    assert!(ok, "{out}");
    assert!(!out.contains("Which rule wins"), "{out}");
}

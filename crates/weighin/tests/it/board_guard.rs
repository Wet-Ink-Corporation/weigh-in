//! The client-repo board guard (decision wi-7c21e4): `board export` writes a board only for a
//! folder whose config exists, parses and chooses the `artifact` board; a broken config stops
//! every command but `hook`, which carries on and says so at `SessionStart`.

use crate::support::{MESSAGE, hook, setup, weighin};
use serde_json::{Value, json};
use std::path::Path;

const CONFIG: &str = ".kb/_ledger/config.json";

/// A value of the wrong type: the kind of typo that used to fall back to the defaults silently.
const BROKEN: &str = r#"{"repo_class":"client","board":"local","store":"../ledger.sqlite3"}"#;

fn raise_one(repo: &Path) {
    let (ok, out) = weighin(
        repo,
        "s1",
        &["raise", "--q", "Keep retention per series?"],
        None,
    );
    assert!(ok, "{out}");
}

/// Overwrites the folder's config with [`BROKEN`].
fn break_config(repo: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(repo.join(".kb/_ledger"))?;
    std::fs::write(repo.join(CONFIG), BROKEN)
}

/// `board export` to its default place and to `--out`; returns each run and whether its file
/// was written.
fn export_both_ways(repo: &Path) -> Vec<(bool, String, bool)> {
    let default = repo.join(".git/weighin/board-state.json");
    let explicit = repo.join("board.json");
    [
        (vec!["board", "export"], default),
        (vec!["board", "export", "--out", "board.json"], explicit),
    ]
    .into_iter()
    .map(|(args, file)| {
        let (ok, out) = weighin(repo, "s1", &args, None);
        (ok, out, file.exists())
    })
    .collect()
}

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn export_is_refused_when_the_board_mode_is_local() {
    let (_tmp, repo) = setup();
    let (ok, out) = weighin(&repo, "s1", &["init", "--class", "client"], None);
    assert!(ok, "{out}");
    raise_one(&repo);

    for (ok, out, written) in export_both_ways(&repo) {
        assert!(
            !ok && out.contains("board mode is `local`") && out.contains("claude.ai"),
            "{out}"
        );
        assert!(!written, "a refused export wrote its state file: {out}");
    }
}

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn export_is_refused_in_a_folder_never_initialised() {
    let (_tmp, repo) = setup();
    raise_one(&repo);

    for (ok, out, written) in export_both_ways(&repo) {
        assert!(
            !ok && out.contains("weighin init --class own") && out.contains("--class client"),
            "{out}"
        );
        assert!(!written, "a refused export wrote its state file: {out}");
    }
}

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn export_is_refused_with_a_broken_config() -> Result<(), Box<dyn std::error::Error>> {
    let (_tmp, repo) = setup();
    let (ok, out) = weighin(&repo, "s1", &["init", "--class", "own"], None);
    assert!(ok, "{out}");
    raise_one(&repo);
    break_config(&repo)?;

    for (ok, out, written) in export_both_ways(&repo) {
        assert!(
            !ok && out.contains("config.json") && out.contains("invalid type"),
            "{out}"
        );
        assert!(!written, "a refused export wrote its state file: {out}");
    }
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn export_writes_the_board_when_the_mode_is_artifact() {
    let (_tmp, repo) = setup();
    let (ok, out) = weighin(&repo, "s1", &["init", "--class", "own"], None);
    assert!(ok, "{out}");
    raise_one(&repo);

    for (ok, out, written) in export_both_ways(&repo) {
        assert!(ok && out.starts_with("wrote "), "{out}");
        assert!(written, "{out}");
    }
}

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn a_broken_config_stops_a_command_and_names_the_file() -> Result<(), Box<dyn std::error::Error>> {
    let (_tmp, repo) = setup();
    break_config(&repo)?;

    let (ok, out) = weighin(&repo, "s1", &["open"], None);

    assert!(!ok, "{out}");
    assert!(
        out.starts_with("weighin: ")
            && out.contains(&format!("cannot parse {}", repo.join(CONFIG).display()))
            && out.contains("invalid type"),
        "{out}"
    );
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn hooks_carry_on_with_a_broken_config_and_session_start_says_so()
-> Result<(), Box<dyn std::error::Error>> {
    let (_tmp, repo) = setup();
    break_config(&repo)?;
    let start = json!({"hook_event_name": "SessionStart", "session_id": "s1", "cwd": repo, "source": "startup"});
    let stop = json!({"hook_event_name": "Stop", "session_id": "s1", "cwd": repo, "last_assistant_message": MESSAGE});

    let started: Value = serde_json::from_str(&hook(&repo, &start))?;
    hook(&repo, &stop);

    let message = started["systemMessage"]
        .as_str()
        .ok_or("SessionStart has a message for the user")?;
    assert!(
        message.contains(CONFIG) && message.contains("defaults"),
        "{message}"
    );
    assert!(!message.contains('\n'), "one line: {message}");
    assert!(
        started["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .ok_or("SessionStart still primes Claude")?
            .contains("```weigh-in"),
        "the prime keeps the convention"
    );
    assert!(
        repo.join(".git/weighin/ledger.sqlite3").exists()
            && !repo
                .parent()
                .ok_or("the repo is in a temp dir")?
                .join("ledger.sqlite3")
                .exists(),
        "Stop captured into the default ledger, not the broken config's store"
    );
    let log = std::fs::read_to_string(repo.join(".git/weighin/hook-errors.log"))?;
    assert_eq!(
        log.lines().filter(|l| l.contains("config.json")).count(),
        1,
        "logged once: {log}"
    );
    Ok(())
}

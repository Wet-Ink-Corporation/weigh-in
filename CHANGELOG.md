# Changelog

One section per release, newest first. The release workflow publishes the section for the
tagged version as the release notes and fails when it is missing, so add it in the commit that
bumps the version. Keep an `Unreleased` section at the top while work lands on `main`.

## Unreleased

- A `weigh-in` block keeps issue references such as `#123` in its values; before, everything
  from ` #123` on was dropped as a comment. A comment is now a ` #` followed by a space or the
  end of the line.
- A `weigh-in` block shown inside an outer fence, such as an example in a four-backtick
  `markdown` fence, is no longer raised as an item; a block after the outer fence closes still is.
- The authority tripwire matches its acronyms (SLA, PII, GDPR, HIPAA, FedRAMP, CJIS) only as
  whole words, so a question about Slack, a slash command or a slate no longer stays open as
  authority-bound.
- A TODO in a block comment followed by code on the same line (`/* TODO: x */ return 0;`,
  `<!-- TODO: x --> <div>`) raises only the comment's text, not the code after it.
- An apostrophe counts as a comment only at the start of a line, so a string such as
  `"can't connect for now"` no longer raises a deferral.
- Questions ending in `#`, `+` or `%` (`C#?`, `C++?`, `100%?`) keep that character when they are
  matched against earlier raises, so `C#` and `C++` questions are no longer merged. Such a
  question now mints a different `wi-` id than before, so one raised before the upgrade and again
  after it in the same session becomes a second item.
- A card's stability counts near the `u32` limit no longer overflow, which wrapped to the wrong
  "earned" verdict for its recommendation.

## 0.5.4 — 2026-10-06

- On a plugin copy with no build for any platform, such as a claude.ai sync of the development
  tree, the launcher now says "this copy has no binaries" and points to the marketplace branch or
  the `weigh-in-plugin` release repository, both in `/weigh-in`'s error and in SessionStart's
  capture-off message (B-18).
- `weighin raise --markers` given no `weigh-in` blocks now fails before it opens the ledger, so it
  no longer leaves an empty ledger in a folder that had none (B-01).
- After `hook-errors.log` is truncated, SessionStart reports every error logged since; before,
  once the log grew past its old length, it skipped the errors before that length and could
  quote a line from the middle. An older build sharing the ledger reads the new record once as
  unreadable and reports the whole log one more time.
- A closed stdout (`weighin open | head`) now ends the command silently with exit 0; before, it
  panicked with exit 101. A broken pipe anywhere else (a closed stderr) is reported as an error
  with exit 1.
- A hook whose output cannot be written now exits 0 and logs the failure to `hook-errors.log`;
  before, it panicked with exit 101. A hook whose input cannot be read or is not JSON logs that
  too, inside a git repository; before, it was ignored without a trace. Outside git such a hook
  still writes nothing, so it leaves no `.weighin/` in the folder it ran in. In Cowork hooks
  still write nothing.
- A command that changes the ledger (`batch`, `board apply`, `scan`, `raise --markers`) whose
  reader closes stdout early still applies every operation, and only its receipts stop.
- A failed `DETACH` of the template database when the store is opened is now returned as an
  error; before, it was ignored.

## 0.5.3 — 2026-10-05

- Each release is also published to `Wet-Ink-Corporation/weigh-in-plugin`, a release-only
  repository whose push starts claude.ai's organization sync, so a release reaches Cowork and
  Claude Code with no zip to upload (B-19). The release needs the secret
  `WEIGH_IN_PLUGIN_DEPLOY_KEY` and refuses to publish without it.

## 0.5.2 — 2026-10-04

- The marker convention SessionStart gives agents now says that a list of calls for the user
  ("Decisions for you", "Your calls") gets one block per call. Agents wrote such lists as prose,
  which capture does not see: since 0.5.0, 1 of the 7 calls in the eval seed was captured (#68).

## 0.5.1 — 2026-10-04

- The board carries at most the newest 200 unacknowledged defaults, with their total, so its
  state stays under the Artifact store's 256 KiB document limit, which about 700 defaults
  overflowed (#19). Acknowledged defaults no longer appear on it; `/weigh-in digest` covers the
  rest, and the board says when the list is capped.
- Read-only commands (`stats`, `open`, `show`, `context`, `digest`, `trailer`, `board export`,
  `prime`) no longer create a ledger in a folder that has none: they report it as empty. Before,
  `weighin --dir <folder> stats` left `.weighin/ledger.sqlite3` and its journal behind, which a
  Cowork connected folder cannot delete. `board export` with no ledger and no `--out` writes
  nothing and says so. A ledger is still created by the first item raised.

## 0.5.0 — 2026-10-03

- Hooks no longer need Git Bash on Windows. `hooks.json` starts the launcher directly, with no
  shell (Claude Code's exec form); on Windows that path resolves to `libexec/weighin.exe`, which
  the package now holds beside the per-target builds. Without Git Bash, Claude Code runs
  shell-form hooks under PowerShell, where `sh` was missing and nothing was captured (#21, B-16).
  `/weigh-in` itself still runs `weighin` through `sh`, so sittings on Windows still need Git Bash.
- `weighin hook` takes the event name `hooks.json` passes after it (`hook SessionStart`), which
  nothing strips when Windows starts the binary directly.

## 0.4.1 — 2026-10-03

- A rerun release no longer packs the failed attempt's release assets into the plugin and the
  `marketplace` branch, and `package-plugin.sh` refuses anything in `libexec/` but the launchers
  and one binary per target (B-15). This release replaces 0.4.0's zip and branch, which carried
  an extra 19 MB `libexec/release-assets/`.
- The install guide turns on auto-update for the `weigh-in` marketplace, which Claude Code
  leaves off for marketplaces that are not official; without it, releases arrive only through
  `/plugin marketplace update weigh-in`.

## 0.4.0 — 2026-10-03

- Claude Code cloud sessions on this repository build `weighin` and offer `/weigh-in` again: the
  SessionStart hook no longer stops when clippy reports the findings temper's baseline records
  (B-13).
- An install guide for users, [`docs/install.md`](docs/install.md): Claude Code from the
  `marketplace` branch, Cowork through a connected checkout and the uploaded zip, updating,
  uninstalling, where the ledger is, reading `hook-errors.log`, and what to do when SessionStart
  says capture is off.
- Each release also publishes a `marketplace` branch: one commit holding the packaged plugin
  with its Linux, Windows and macOS binaries, so Claude Code installs without Rust from
  `Wet-Ink-Corporation/weigh-in#marketplace` (D-0009, B-14). CI builds the same branch from
  every pull request.

## 0.3.0 — 2026-10-03

- Repository setup for ongoing development (v1 plan, phase 0): CI also runs on macOS, checks the
  plugin package and the changelog, and runs `cargo deny`; release notes come from this file and
  a release tag must be on `main`; pull request template, CODEOWNERS, a pre-push guard for
  `archive/*` branches, and a SessionStart hook for Claude Code cloud sessions.
- `weighin.cmd` runs the newer of the msvc and gnu builds, as the `sh` launcher does, so a stale
  native Windows build no longer shadows a fresh cross-build (B-05).
- The ledger carries a schema version (`PRAGMA user_version`, now 1). Ledgers from 0.1.x (WAL)
  and 0.2.x (TRUNCATE) are stamped on first open and read as before; a ledger stamped by a newer
  build is refused, untouched, with a message to upgrade `weighin` (B-10).
- Capture is never off without a word. When the plugin has no binary for the platform,
  SessionStart shows a one-line message saying capture is off and how to fix it; when hooks have
  logged errors to `hook-errors.log` since the last session, SessionStart shows how many and the
  last one. Hooks still always exit 0, and say nothing in Cowork (B-11).
- This repository uses Weigh-In on itself (D-0010): `.kb/` is committed, and Claude Code cloud
  sessions here build the binary and load the skill and hooks from the checkout (B-12).

## 0.2.2 — 2026-10-01

- Paths a hook or agent spells differently from git's toplevel (8.3 short names, symlinked temp
  dirs, `..`, files not written yet) are matched to the repository after resolving them, and
  `at` anchors keep their line and column (B-08).
- rustdoc builds clean under `-D warnings`.
- CI runs the gate on Linux and Windows for every push and pull request (B-07).

## 0.2.1 — 2026-10-01

- First release from `Wet-Ink-Corporation/weigh-in`: the CLI, the plugin (`/weigh-in`), the
  research and the decision records, seeded as one commit (B-06). Earlier history is kept
  outside GitHub.

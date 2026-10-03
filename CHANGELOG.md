# Changelog

One section per release, newest first. The release workflow publishes the section for the
tagged version as the release notes and fails when it is missing, so add it in the commit that
bumps the version. Keep an `Unreleased` section at the top while work lands on `main`.

## Unreleased

- The board carries at most the newest 200 unacknowledged defaults, with their total, so its
  state stays under the Artifact store's 256 KiB document limit, which about 700 defaults
  overflowed (#19). Acknowledged defaults no longer appear on it; `/weigh-in digest` covers the
  rest, and the board says when the list is capped.

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

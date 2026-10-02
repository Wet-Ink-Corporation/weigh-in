# Changelog

One section per release, newest first. The release workflow publishes the section for the
tagged version as the release notes and fails when it is missing, so add it in the commit that
bumps the version. Keep an `Unreleased` section at the top while work lands on `main`.

## Unreleased

- Repository setup for ongoing development (v1 plan, phase 0): CI also runs on macOS, checks the
  plugin package and the changelog, and runs `cargo deny`; release notes come from this file and
  a release tag must be on `main`; pull request template, CODEOWNERS, a pre-push guard for
  `archive/*` branches, and a SessionStart hook for Claude Code cloud sessions.
- The ledger carries a schema version (`PRAGMA user_version`, now 1). Ledgers from 0.1.x (WAL)
  and 0.2.x (TRUNCATE) are stamped on first open and read as before; a ledger stamped by a newer
  build is refused, untouched, with a message to upgrade `weighin` (B-10).

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

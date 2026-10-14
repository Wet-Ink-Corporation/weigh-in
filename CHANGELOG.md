# Changelog

One section per release, newest first. The release workflow publishes the section for the
tagged version as the release notes and fails when it is missing, so add it in the commit that
bumps the version. Keep an `Unreleased` section at the top while work lands on `main`.

## Unreleased

- `scripts/weekly-stats.sh` finds the organization-synced plugin when the home folder's path has a
  space in it, and takes the newest synced copy that has a built binary, not the first one found.

## 0.6.2 — 2026-10-13

Fixes for what Ryan's checks of 0.6.1 found in the sitting: Claude Projects tasks (#126) and the
Windows desktop app (#127). Capture (markers, the TODO scan, the Stop and PostToolUse hooks) is
unchanged, so the #23 pilot still measures 0.6.0's capture behaviour (B-23).

- In a Claude Projects task, `/weigh-in` asks each card with the task's tappable decision card
  instead of printing the cards as text, and records each answer as it arrives. A one-way card
  says nothing is recorded until you pick, and an unanswered card is never recorded (#126).
- In a Projects task that worked in a connected folder on your computer, the sitting records in
  that folder's ledger through the computer's shell, as Cowork does, and carries over the items
  the task's hooks captured in the cloud workspace (D-0018, #126).
- On Windows, inside a git repository, `SessionStart` copies the `weighin` binary into the
  repository's git dir (`.git/weighin/bin/<version>/`) and names it in the session's context.
  The skill uses that copy when its shell can't run the plugin's own binary. This was the case in the Windows
  desktop app, where every sitting recorded nothing (B-26, #127).
- When the `weighin` binary can't run, the sitting says nothing was recorded, shows the command
  and its error, and keeps every item open. Before, it could report items as dismissed or
  decided in chat although nothing reached the ledger (#127).
- A decision atom's summary no longer doubles its punctuation when the chosen option's premise
  or its if-wrong text ends with a full stop (`small., … conflict..` now reads
  `small, … conflict.`). Atoms already written keep their text (wi-56bd73).

## 0.6.1 — 2026-10-11

The client-repo board guard and test gaps from the 2026-10-06 status audit. Capture (markers,
the TODO scan, hooks) is unchanged, so the #23 pilot still measures 0.6.0's capture behaviour
(B-23).

- `weighin board export` refuses, and writes no state file, unless the folder's
  `.kb/_ledger/config.json` chose the `artifact` board. A `local` or `off` mode, a folder never
  initialised and a config that does not parse each stop it with a message saying what to do;
  before, it only printed a note and a client repository never initialised published as if it
  were your own (wi-7c21e4).
- A `.kb/_ledger/config.json` that cannot be read or parsed is an error naming the file and the
  fault for every command, where before it was silently replaced by the defaults, dropping a
  `store` that keeps the ledger out of a synced folder. Hooks still never fail a session: they
  run on the defaults, capturing into the default ledger, and every `SessionStart` tells you the
  config is broken and where captured items go until it is fixed.
- A context pack reads a `.kb` atom's `id`, `status` and `title` from its frontmatter only, so a
  body line such as `status: open` is no longer shown as the atom's status.

## 0.6.0 — 2026-10-10

Released before the two weeks of daily use (#23) open, so the pilot measures one capture
behaviour (wi-7c3aa0).

- A `weigh-in` block whose `did` starts with the word "blocked" followed by an explanation
  (`did: blocked; left the guard as it is`) stays an open question; before, only a bare
  `did: blocked` did, and the rest were logged as calls the agent made (wi-64c8ea).
- The TODO scanner ignores comment markers inside double-quoted strings, such as test fixtures
  holding `"// TODO: …"`; one session's ledger got 212 such items. Single quotes are not tracked,
  so lifetimes, char literals and apostrophes hide nothing; a stray `"` in prose hides the rest
  of its line (wi-52e471).
- A single-segment `scan.exclude` entry such as `node_modules/` or `target/` matches at any depth,
  as `.gitignore` treats a bare name, so a nested `packages/app/node_modules/` is no longer
  scanned. Entries with more segments stay anchored at the root (wi-e038ef).
- `weighin journal` prints every ledger event as JSON lines, and `weighin stats` now folds the
  ledger together with every journal committed under `.kb/_ledger/journal/`, counting an event
  held twice only once. Cloud sessions on this repository write such a journal as they go, so
  their presentations, dismissals, deferrals and defaults count in the pilot's numbers even
  after the container is gone (B-21).
- `scripts/weekly-stats.sh` reads a folder that has committed journals but no ledger, and adds
  their hook errors to the count. It also finds the organization-synced install, and stops with
  one message when it finds no binary, where before every row read "stats failed".
- A `weigh-in` block keeps issue references such as `#123` in its values; before, everything
  from ` #123` on was dropped as a comment. A comment is now a ` #` followed by a space or the
  end of the line.
- A `weigh-in` block shown inside an outer fence, such as an example in a four-backtick
  `markdown` fence, is no longer raised as an item; a block after the outer fence closes still is.
  An outer fence with no closing line anywhere after it, or one indented four spaces or more
  (indented code), does not hide the blocks after it. A three-backtick `weigh-in` block after an
  unclosed three-backtick example is still read as that example's content, as Markdown renders it.
- The authority tripwire matches its acronyms (SLA, PII, GDPR, HIPAA, FedRAMP, CJIS) only as
  whole words, so a question about Slack, a slash command or a slate no longer stays open as
  authority-bound.
- A TODO in a block comment followed by code on the same line (`/* TODO: x */ return 0;`,
  `<!-- TODO: x --> <div>`) raises only the comment's text, not the code after it.
- An apostrophe counts as a comment only at the start of a line, so a string such as
  `"can't connect for now"` no longer raises a deferral.
- Questions ending in `#`, `+` or `%` (`C#?`, `C++?`, `100%?`) keep that character when they are
  matched against earlier raises, so `C#` and `C++` questions are no longer merged. An item
  raised before the upgrade is still recognised when the same question is raised again.
- A card's stability counts near the `u32` limit no longer overflow, which wrapped to the wrong
  "earned" verdict for its recommendation.
- The board's "Copy as prompt" now copies a request the next `/weigh-in` sitting acts on
  (`/weigh-in decide wi-x: A (note: …)`, `/weigh-in defer wi-x — trigger: …; resolve by: …;
  default: A`); before, its defer text passed the trigger as `--flip` and left out the required
  `--trigger`, and the skill had no such arguments. A note on a deferral is kept as its reason.
  In Dismiss and Not my call, the button is off: it used to copy a decide request. It also
  needs what the Record buttons need: a picked option to copy a decision (before, with nothing
  picked it copied the recommendation as the owner's choice), and a trigger and resolve-by to
  copy a deferral.
- `/weigh-in` quotes the session id it passes to `weighin`, so an empty one no longer fails every
  call with "a value is required for '--session'".
- In Cowork, the advice to move a synced folder's ledger keeps the folder's class and board mode,
  or uses `client` and `local` when the folder has no config; before, it suggested `--class own`,
  which turned a client folder into an own one with an artifact board.
- `scripts/weekly-stats.sh` ignores `$WEIGHIN_STORE` and reads each folder through its own ledger;
  before, every folder reported that one ledger and the pooled row counted it once per folder.
- `scripts/build-plugin-bins.ps1` stops when `cargo build` fails; before, it copied the previous
  `weighin.exe` into the plugin.
- `scripts/package-plugin.sh` leaves a leftover `plugin/libexec/weighin.exe` out of the zip when
  there is no Windows build under `plugin/libexec/<target>/`, and says so; before, it shipped it
  unchecked.
- `decide` writes `→ Weigh-In <id>: decided <option> (<date>)` only into markdown (`.md`, `.mdx`,
  `.markdown`) and `.txt` files, and not when the option holds a control character or line
  separator; before, it appended the note to the source line of any file, which could break the
  code. Code files are no longer annotated: the decision stays in the ledger, its atom and the
  commit trailer, and the receipt says `not annotated <file> (code file: the decision is in the
  ledger)`.
- In `batch`, `@N` always names the Nth raise in the array. A raise that fails, or cannot be
  parsed, keeps its place: a ref to it is refused, and later refs still reach their own items;
  before, every later ref moved to the next item raised, so an op could act on the wrong item.
- `weighin init` changes only what its flags name: `init --ledger <path>` or `init --owner <name>`
  on a client repo keeps its class and its `local` board; before, any `init` without `--class`
  reset the repo to `own` on the `artifact` board. A first `init` is still `own` on `artifact`.
- `weighin --dir <folder> scan <path>` reads a relative path under `<folder>`, as it already
  anchored it; before, it read the path against the shell's own directory, so it failed or raised
  another file's questions into the folder's ledger.
- A command that changes the ledger creates it only when it first writes: `decide`, `defer`,
  `dismiss` or `reopen` that names no item, an empty `batch`, or `board apply` with no answers no
  longer leaves an empty ledger in a folder that had none (B-01).
- Deciding a deferred one-way item now closes its tracking atom in
  `.kb/_intake/open_questions/`: `status: superseded`, `superseded_by` and `resolution_ref`
  naming the decision atom, a settled summary and a Resolution section, and the receipt says
  `superseded <path>`; before, it stayed `status: proposed` and read as still open.
- `weighin context` and `open --with-context` no longer panic on an item whose `at:` line number
  is near the largest integer; the pack is built with an empty excerpt.
- `board apply <dir>` and the `.kb` matches in context packs no longer follow symlinked
  directories, so a link back up the tree no longer reads the same answers many times over or
  hangs the command.

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

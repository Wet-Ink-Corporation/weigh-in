# Weigh-In

A decision ledger for agent sessions. Agents mark what they assumed, deferred or could not
decide; hooks capture those marks and the open questions they write into documents; `/weigh-in`
filters them hard and settles the ones that need you in one sitting, at the end of the chat
that raised them. Every decision lands as a `.kb` atom that Redkiln's `kb-ingest` promotes.

Slice 1 of the design in *Settle agent questions while the context is warm* (2026-09-26).
It stands on its own until redkiln-rs is ready to receive it.

## How it works

| Stage | Where | Model tokens |
|---|---|---|
| Capture: `weigh-in` blocks in the agent's last message | `Stop` / `SubagentStop` command hook | 0 |
| Capture: "Open questions" bullets and new TODOs in written files | `PostToolUse` command hook | 0 |
| Convention and open counts | `SessionStart` command hook | ~150 per session |
| Sweep, five filters, grounding, cards | `/weigh-in` skill, in the chat that raised them | one sitting, 3–4 requests |
| Record | `weighin` CLI: event, atom in `.kb/_intake/`, source annotation | 0 |

The ledger is an event log in happenstance's SQLite schema at
`<git-common-dir>/weighin/ledger.sqlite3`: shared by every worktree of the repo, never
committed. Writes are conditioned (DCB): raising an item is idempotent, and two sessions
cannot both decide the same item. Decisions reach git as atoms and as a
`Weigh-In: <id> decided <X>` commit trailer.

Where the ledger lives, first match wins: `--store <file>`; `$WEIGHIN_STORE`; `store` in
`.kb/_ledger/config.json` (set with `init --ledger <path>`, relative to the folder or absolute);
the git common dir; outside git, `<folder>/.weighin/`. It runs under a TRUNCATE journal, not
happenstance's WAL: WAL and SQLite's default both delete files beside the database, and Cowork's
connected folders refuse deletes. For a folder synced by OneDrive or Dropbox and used from more
than one computer, point `store` outside the synced tree.

## Cowork

Cowork runs skills and hooks in a cloud workspace whose project folder is scratch, deleted with
the session; your files are on your computer, behind its shell. So in Cowork:

- The skill runs `weighin` through your computer's shell, against the connected folder you
  worked in. With public releases, it downloads the Linux binary of its own release from
  GitHub on first use in a chat, into the shell's per-chat scratch space (outside every
  connected folder), and checks it against the SHA-256 stamped into the skill. Otherwise, and
  while this repository is internal, it uses the copy in a connected folder holding a checkout
  of the `marketplace` branch (`plugin/libexec/`, [install](docs/install.md#cowork)); a checkout
  of `main` has no binaries (B-18). Failing both, it settles in chat and hands you Y-statements.
- Hooks write nothing (`CLAUDE_CODE_ENTRYPOINT=remote_cowork`): `SessionStart` still teaches the
  marker convention and `Stop` still nudges on one-way markers. Edits to your folder go through
  the computer's shell, so `PostToolUse` never sees them; the sitting scans the files the
  conversation changed instead.
- Decisions land as atoms in `<folder>/.kb/_intake/decisions/`, and in the folder's own ruling
  or decision log when it keeps one.

## Docs

- [`docs/research/report.md`](docs/research/report.md): the research and the design, with dated amendments.
- [`docs/decisions/`](docs/decisions/README.md): every decision so far, including the board sitting and the calls made while building.
- [`docs/install.md`](docs/install.md): installing, updating and removing a release.
- [`docs/roadmap.md`](docs/roadmap.md): what is not yet verified, known gaps, and slice 2.

## Install

To use Weigh-In, install a release: [`docs/install.md`](docs/install.md) covers Claude Code
(from the `marketplace` branch, no Rust needed), Cowork, updates, uninstalling, where the ledger
is and what to do when capture is off. The rest of this section builds it from source.

```sh
# Linux / macOS / cloud: static binaries into plugin/libexec/<target>/
./scripts/build-plugin-bins.sh            # linux (musl) and windows (mingw) cross-builds
# Windows, native:
pwsh scripts/build-plugin-bins.ps1        # x86_64-pc-windows-msvc
```

Then in Claude Code: `/plugin marketplace add <path to this repo>` and
`/plugin install weigh@weigh-in`. Type `/weigh-in` everywhere. Cowork and claude.ai list plugin
skills by the skill's own name (`/weigh-in`); Claude Code lists them as `/<plugin>:<skill>`
(`/weigh:weigh-in`) but also resolves the bare name. A repo needs no setup; run
`/weigh-in init client` first in client repos, which sets the board to `local`. From Claude Code's `!` prompt on Windows, which is Git Bash, use
forward slashes: `! sh D:/repos/weigh-in/plugin/libexec/weighin init --class own`.

After pulling a new version, rebuild every binary you keep: the launcher prefers the native
`x86_64-pc-windows-msvc` build over the `gnu` one unless the `gnu` one is newer, and an old
build next to a new one would reopen the ledger under its own journal mode.

Cowork and claude.ai take plugins from your account, not from a local marketplace, and the
binaries are not in git. An organization that syncs `Wet-Ink-Corporation/weigh-in-plugin`, which
each release fills (B-19), gets every release; otherwise upload `weigh-in-plugin-<version>.zip`
from the GitHub release under Customize > Plugins. A plugin saved to your account also shows up
in Claude Code; if you then see `/weigh-in` twice, remove one of the two installs.

## Releasing

Bump `version` in `Cargo.toml` and `plugin/.claude-plugin/plugin.json`, move `## Unreleased` in
`CHANGELOG.md` to the new version, merge to `main`, then push the tag `v<version>`. Push only the
tag: the workflow creates the GitHub release itself, refuses a tag that is not on `main` or has no
changelog entry, and a rerun updates the release it made. The release workflow tests, builds the
Linux (musl), Windows (msvc) and macOS binaries, packages the plugin with its skill stamped to that
Linux build's SHA-256, publishes the release with the zip, the raw binaries and `SHA256SUMS`, and
then replaces the `marketplace` branch with one commit holding that zip's plugin and
`marketplace.json`, which Claude Code installs from (D-0009, `scripts/marketplace-branch.sh`), and
pushes the same commit to the default branch of `Wet-Ink-Corporation/weigh-in-plugin` for the
organization sync (B-19). The zip and the Linux binary must come from the same build, which is why
the release is made only there; `scripts/package-plugin.sh` run locally gives a zip that matches
only a local build. The download needs public release assets: while the repository is internal
the workflow packages with `--no-download`, and so should a local build
(`scripts/package-plugin.sh --no-download`); the skill then uses the binary in a connected
checkout of the `marketplace` branch.

## Commands

```
weighin [--store <file>] [--dir <folder>] [--session <id>] <command>
weighin prime                            the marker convention and open counts
weighin init [--class own|client] [--board artifact|local|off] [--board-url <url>] [--owner <who>]
             [--ledger <path>]
weighin open [--repo | --all] [--with-context] [--top <n>] [--json]
                                         open items, ranked (one-way, blocking, authority first);
                                         `--all` lists closed ones too
weighin show <id> [--json]               one item in full
weighin context <id> [--json]            script-built evidence, ≤600 tokens
weighin raise --q <question> … | --markers
                                         raise by hand, or every `weigh-in` block on stdin
weighin batch [--file <ops.json>]        raise / triage / ground / present / decide / defer /
                                         default / dismiss / reopen / supersede / link / ack
weighin decide <id> <option> [--note] [--via <channel>]
                                         record a decision, write the atom, annotate the source
weighin defer <id> --flip --trigger --by --default [--reason]
weighin dismiss <id> <why> [--note]      not-useful | recoverable | moot | already-decided:<ref> |
                                         duplicate-of:<id> | not-my-call:<who>
weighin reopen <id> [--note]             reopen a decided, defaulted or dismissed item
weighin digest [--json]                  defaults taken without you
weighin stats [--json]                   action rate, not-useful rate, overrides, time to decide (ledger + committed journals)
weighin journal                          every ledger event as JSON lines (cloud sessions commit it, B-21)
weighin trailer <id>                     the commit trailer for a decided item
weighin scan <path>…                     backfill from existing documents
weighin board export [--out <file>]      write the board state document
weighin board apply <path>               apply board answers (a file or a directory)
weighin hook                             hook entry point (reads the hook JSON on stdin)
```

## Guards the CLI enforces

- A card has 2–4 options; the recommendation cannot fail a MUST or be flagged evasive.
- One-way and unsure doors are never defaulted; authority-bound items default only on expiry.
- A deferral names the fact it waits for, a trigger, a resolve-by and a default.
- An authority-sounding item (compliance, isolation, retention, security, migration, …) is
  dismissed as "already decided" only against an accepted `kb-…` atom or the user's own words.
- An agent's two-way "judgment" marker with authority words stays open instead of defaulting.

## Measured in this build

Measured on 0.5.4 (2026-10-07). Items marked **pending** are v1 exit criteria
([v1 plan](docs/v1-plan.md), Phase 3) that have not been measured yet.

- **Tests and lints.** 245 tests on `main` after the 2026-10-07 review (102 core unit tests, 22
  core property tests, 7 core doctests, 70 CLI unit tests, 44 CLI integration tests including the
  eval scorer; one more end-to-end test runs on Windows only). At the 0.5.4 tag the count was
  180. CI runs them, fmt and
  rustdoc on Linux, Windows and macOS; `cargo deny`, the plugin package check and temper's lint
  ratchet run on Linux. The temper baseline is 0 findings (721 remediated in #82). Atoms pass
  Redkiln's `validate --kb`, with a rejected negative control.
- **Reviews.** An independent review at 0.1 found eleven defects; all are fixed, each with a
  regression test. The `/code-review` of everything since 0.2.2 (#26) found 14 candidates.
  Seven were verified and fixed: two in the CLI, five in scripts and workflows. One was refuted
  by the B-16 spike, and six were left with reasons on #26.
- **Upgrades.** A 0.1.x WAL ledger and a 0.2.2 TRUNCATE ledger open, are stamped with the schema
  version and read back (fixture tests, B-10). Cowork's refused deletes, reproduced with a shim
  that refuses `unlink`, break 0.1.1 with `SQLITE_IOERR_DELETE` (2570). From 0.1.2 the store
  creates, raises and decides with no delete or rename in the folder.
- **Capture precision** (eval seed, #22): 70 real end-of-turn messages from cloud sessions on this
  repository, 19 labelled `capture`, 12 captured. Precision is 12 of 12 (100%, bar 80%, in force
  since 10 items are captured, wi-a9df17). Recall is 1 of 7 before the 0.5.2 convention line and
  11 of 12 after it.
  - **Pending:** the labels await Ryan's review (#22).
- **Sitting cost** (D-0016): about 3 × the session's context at the cache-read price, plus about
  5,600 output tokens of cards. That is about $0.10–0.15 on Sonnet 5.5 after a short session and
  about $0.47 after a 585k-token one. Earlier live runs (`--plugin-dir`) cost $0.42–0.57, against
  the design's $0.28.
- **Surfaces.**
  - Windows: installed from the `marketplace` branch with no toolchain, hooks fired and a sitting
    decided an item (V7, #13). Exec-form hooks run with or without Git Bash (B-16, spiked on all
    three platforms).
  - Board: in the desktop Code tab (V6). The `db` store's document limit is 262,144 bytes, and a
    board capped at 200 defaults measures 59,678 bytes at 1,000 (V5). Editor links from the
    board are not supported (V4).
  - Cowork: the zip-synced plugin keeps the launcher's executable bit and shows the SessionStart
    line.
  - Releases: 0.5.4 reaches claude.ai's organization sync through `weigh-in-plugin` (B-19), with
    Linux musl, Windows msvc and macOS arm64 binaries and `libexec/weighin.exe`. A copy synced
    from the development tree has none (#76); from 0.5.4 the launcher says so (B-18).
  - **Pending:** Ryan's Windows check of the synced `weigh-in-plugin` copy, which closes #76.
  - **Pending:** the Cowork session id and the Cowork half of V6 (V1, #16); a marker captured on
    Windows without Git Bash (V7, #21).
- **Use.** **Pending:** two weeks and 30 presented items of real sittings, whichever comes later
  (#23, from 2026-10-05).
  These give the action rate (target at least 60%), the not-useful rate (at most 10%, or
  per-category suppression comes into v1, #25), overrides and time to decide. `weighin stats`
  and `scripts/weekly-stats.sh` produce them.

## Open items

Verification spikes (V1–V7), known gaps and slice 2 are tracked in [`docs/roadmap.md`](docs/roadmap.md).

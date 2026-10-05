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
  while this repository is internal, it uses the copy in a connected folder holding this
  repository (`plugin/libexec/`); failing both, it settles in chat and hands you Y-statements.
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

Bump `version` in `Cargo.toml` and `plugin/.claude-plugin/plugin.json`, commit to `main`, then
push the tag `v<version>`; push only the tag, since the workflow creates the GitHub release and
stops if one already exists. The release workflow tests, builds the Linux (musl), Windows (msvc)
and macOS binaries, packages the plugin with its skill stamped to that Linux build's SHA-256,
publishes the release with the zip, the raw binaries and `SHA256SUMS`, and then replaces the
`marketplace` branch with one commit holding that zip's plugin and `marketplace.json`, which
Claude Code installs from (D-0009, `scripts/marketplace-branch.sh`), and pushes the same commit
to the default branch of `Wet-Ink-Corporation/weigh-in-plugin` for the organization sync (B-19).
The zip and the Linux binary must come from the same build, which is why the release is made only there;
`scripts/package-plugin.sh` run locally gives a zip that matches only a local build. The
download needs public release assets: while the repository is internal the workflow packages
with `--no-download`, and so should a local build (`scripts/package-plugin.sh --no-download`);
the skill then uses the binary in a connected folder holding this repository.

## Commands

```
weighin [--store <file>] [--dir <folder>] [--session <id>] <command>
weighin init --class own|client [--board artifact|local|off] [--ledger <path>]
weighin open [--repo] [--with-context]   open items, ranked (one-way, blocking, authority first)
weighin context <id>                     script-built evidence, ≤600 tokens
weighin batch < ops.json                 raise / triage / ground / present / decide / defer /
                                         default / dismiss / reopen / supersede / link / ack
weighin decide <id> <option> [--note]    record a decision, write the atom, annotate the source
weighin defer <id> --flip --trigger --by --default
weighin dismiss <id> <why>               not-useful | recoverable | moot | already-decided:<ref> |
                                         duplicate-of:<id> | not-my-call:<who>
weighin digest                           defaults taken without you
weighin scan <path>…                     backfill from existing documents
weighin board export | apply <dir>       exchange with the Artifact board
weighin stats                            action rate, not-useful rate, overrides, time to decide
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

- 57 tests on Linux, 55 on Windows (31 core; 18 store, git and paths, two of them Unix-only
  symlink cases; 8 end to end), clippy and rustdoc clean, run by CI on both. Atoms pass Redkiln's
  `validate --kb` (with a rejected negative control). An independent review found eleven
  defects (a hook panic on non-ASCII text, stale board answers, batch parsing, id collisions,
  re-raised settled questions, out-of-repo scans, CRLF rewrites, stray stores, a launcher loop,
  the `.cmd` exit code, YAML escaping); all are fixed and each has a regression test.
- Live Claude Code sessions (Sonnet, `--plugin-dir`): the agent emitted a `weigh-in` block
  unprompted from the SessionStart convention; hooks captured markers and document questions;
  a sitting took 3–4 requests and cost $0.42–0.57, against the design's $0.28
  estimate. Re-measured on 0.5.0 (D-0016): the cost is mostly the three requests re-reading the
  session's context; the cards themselves are about 5,600 output tokens. So a sitting costs about
  3 × the context at the cache-read price plus that output: about $0.10–0.15 on Sonnet 5.5 after a
  short session, about $0.47 after a 585k-token one.
- Cowork's refused deletes, reproduced with a shim that refuses `unlink`: 0.1.1 fails with
  `SQLITE_IOERR_DELETE` (2570) exactly as reported; 0.1.2 creates, raises and decides with no
  delete or rename in the folder (strace), recovers the files 0.1.1 left behind, and converts a
  0.1.1 WAL ledger even where deletes are refused (its checkpointed `-wal` is renamed aside).
- A static Linux (musl) binary runs from a local plugin folder in a Claude Code cloud session.
  Binaries are not committed (`plugin/libexec/*/` is ignored): a git or marketplace install needs
  the release workflow's zip, or `scripts/build-plugin-bins.*` run once after cloning.

## Open items

Verification spikes (V1–V7), known gaps and slice 2 are tracked in [`docs/roadmap.md`](docs/roadmap.md).

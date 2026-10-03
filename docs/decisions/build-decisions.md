# Decisions taken while building and shipping, 2026-09-27 to 2026-10-02

These came up in chat rather than on the board. `D-` entries are Ryan's calls; `B-` entries are
build calls Claude made, either forced by a platform constraint or as two-way choices, and are
open to challenge like any other default.

## D-0002

**Cowork runs the ledger through the computer's shell.** 2026-09-28 · amends the Cowork half of
[wi-b4c1d8](wi-b4c1d8.md) · provenance: Ryan passed on a Cowork session's findings and asked for
them to be addressed; built as proposed.

Context. In Cowork, skills and hooks run in a cloud workspace whose project folder is scratch and
is deleted with the session; the user's files sit behind the computer's shell. wi-b4c1d8 had
Cowork use the board store with no binary, because no binary was thought reachable there. The
shell on the computer can run the Linux binary against a connected folder.

Decision. When the skill can reach a `weighin` binary from the computer's shell, Cowork runs it
there against the folder the conversation worked in, so a Cowork folder keeps the same ledger,
guards and `.kb` atoms as a repo in Claude Code. Without a binary the skill settles in chat and
hands over Y-statements; the board remains the fallback. The sitting scans the files the
conversation changed, because `PostToolUse` never sees edits made through the computer's shell.
Decisions also go into the folder's own ruling or decision log when it keeps one.

## D-0003

**Cowork gets the binary by downloading it from a GitHub release.** 2026-09-28 · provenance: Ryan
chose option 1 of 3.

Options weighed: (1) download from a GitHub release into the shell's per-chat scratch space,
checked against a SHA-256 stamped into the skill; (2) copy the binary into each work folder,
which puts a 4.5 MB executable into synced client folders; (3) one dedicated Weigh-In folder on
the computer, requested through the folder-access prompt each chat.

Decision. Option 1. `scripts/package-plugin.sh` stamps the version and the Linux binary's
SHA-256 into the zipped skill; the release workflow builds, stamps and publishes from one build.
If the download fails or does not match, the skill uses a copy in a connected folder.

Status. Built and tested against a local copy of a release; dormant. The repository is internal
([D-0007](#d-0007)) and the computer's shell has no GitHub login, so zips are packaged with
`--no-download` and Cowork uses the copy in a connected `weigh-in` folder.

## D-0004

**The command is `/weigh-in`: plugin `weigh`, skill `weigh-in`.** 2026-09-28 · provenance: Ryan.

History. `/weigh-in:weigh-in` was unwieldy in Claude Code. `/weigh:in` (plugin `weigh`, skill
`in`) read well in Claude Code but showed as a bare `/in` in Cowork, which lists plugin skills by
the skill's own name.

Decision. Name the skill so it stands alone, as other plugins do (`inbox-zero` with `iz-rules`,
`create-cowork-plugin`, `pdf`). Cowork shows `/weigh-in`; Claude Code lists `/weigh:weigh-in` and
also resolves the bare `/weigh-in` (checked with `claude -p`). The install id stays
`weigh@weigh-in`.

## D-0005

**One skill with arguments; no split yet.** 2026-09-28 · provenance: Ryan.

The occasional routes (`init`, `digest`, `board`, `sweep`, `stats`) stay arguments of
`/weigh-in` rather than separate `/weigh-in-digest`-style skills. A split would make them visible
in both menus and cheaper to run, since they would not load the full sitting instructions; it is
not wanted now.

## D-0006

**Distribute through the local marketplace for now.** 2026-09-28 · provenance: Ryan.

Claude Code installs from the marketplace in this repository (`/plugin install weigh@weigh-in`).
Cowork, which only takes plugins from the account, gets zips uploaded under Customize > Plugins.

## D-0007

**Weigh-In lives in `Wet-Ink-Corporation/weigh-in`, internal.** 2026-10-01 · provenance: Ryan
created the repository and a local clone.

Consequence. Release assets are not public, so the Cowork download ([D-0003](#d-0003)) stays off
until the repository or its releases are public; see [B-06](#b-06).

## D-0008

**v1 means daily use by Ryan.** 2026-10-02 · provenance: Ryan, answering P-1 of the
[v1 plan](../v1-plan.md#decisions).

v1 is live when Ryan uses `/weigh-in` daily on his own machines and in Cowork. Installing for
other people, a public plugin, and an owner default other than `ryan` are not v1 work.

## D-0009

**Claude Code installs from a built marketplace in this internal repository.** 2026-10-02 ·
amends [D-0006](#d-0006) · provenance: Ryan, answering P-2 of the [v1 plan](../v1-plan.md#decisions).

The release workflow publishes the built plugin, binaries included, to a `marketplace` branch
here, and Claude Code installs from `Wet-Ink-Corporation/weigh-in#marketplace` with the
machine's git credentials. The local marketplace on `main` stays for development. Cowork still
gets the uploaded zip.

## D-0010

**This repository uses Weigh-In on itself.** 2026-10-02 · provenance: Ryan, answering P-4 of
the [v1 plan](../v1-plan.md#decisions).

`init --class own` here, `.kb/` committed, and new decisions go through `/weigh-in`. The
hand-kept records in this folder stay as the history before that.

## D-0011

**Open work lives in GitHub Issues.** 2026-10-02 · provenance: Ryan, answering P-5 of the
[v1 plan](../v1-plan.md#decisions).

Verification items, known gaps and plan tasks are issues under a `v1.0` milestone;
`docs/roadmap.md` summarises and links them.

## D-0012

**Release assets stay internal for v1.** 2026-10-02 · provenance: Ryan, answering P-3 of the
[v1 plan](../v1-plan.md#decisions).

No second repository and no public assets. Claude Code installs from the internal `marketplace`
branch (D-0009); Cowork uses the uploaded zip and a connected checkout of that branch, so the
Cowork download ([D-0003](#d-0003)) stays dormant. Going public is after v1, by cleaning up the
research and making this repository public; there is still one repository.

## D-0013

**No slice 2 work in v1, unless the not-useful rate misses.** 2026-10-02 · provenance: Ryan,
answering P-6 of the [v1 plan](../v1-plan.md#decisions).

v1 is slice 1 made dependable. If two weeks of daily use show more than 10% of items dismissed
`not-useful`, per-category suppression comes into v1, since it is the design's fix for that miss.

## D-0014

**This repository's ledger stays at the default location.** 2026-10-02 · provenance: Ryan,
choosing between keeping the default, a home-directory ledger, committing the ledger, and
deferring [D-0010](#d-0010) to Phase 2.

`.kb/_ledger/config.json` sets no `store`, so the ledger is `<git common dir>/weighin/` ([B-02](#b-02)).
On Ryan's clones it lasts; in Claude Code cloud sessions it goes with the container. Decisions
survive either way as committed `.kb` atoms; what a container takes with it is the event history
and anything left open, deferred or defaulted, so cloud sessions settle everything before they
end. Accepted until Phase 2's installs, and noted as a known gap.

## D-0015

**Hooks work on Windows without Git Bash in v1.** 2026-10-03 · provenance: Ryan, choosing between
exec-form hooks now, requiring Git Bash for v1, and exec-form hooks with a shell-neutral skill ·
closes the hook half of [#21](https://github.com/Wet-Ink-Corporation/weigh-in/issues/21).

Claude Code without Git for Windows runs shell-form hooks under PowerShell, where `sh` is not
found, so capture was silently off. The hooks move to exec form ([B-16](#b-16)) in 0.5.0. The
sitting still runs through `sh`, so `/weigh-in` on Windows still needs Git Bash; a shell-neutral
skill was not chosen.

## D-0016

**v1 records the measured sitting cost instead of trimming the output.** 2026-10-03 · provenance:
Ryan, choosing between measuring then recording, trimming and recording, and trimming only ·
closes [#24](https://github.com/Wet-Ink-Corporation/weigh-in/issues/24).

A sitting's cost is set by the session it ends, not by the cards. Measured from the transcript of
the session that chose this (0.5.0, Opus 5.5, three requests, two cards, four swept items):

| Request | Cache read | Cache write | Output |
|---|---:|---:|---:|
| Batch 1 (sweep, triage, two cards) | 583,694 | 4,750 | 4,392 |
| AskUserQuestion | 588,444 | 4,807 | 972 |
| Batch 2 (decisions) | 588,444 | 15,516 | 224 |

That is about $0.59 at Opus 5.5's prices ($4 in, $20 out, $0.20 cache read; cache writes taken as
1.25 × input) and about $0.47 at Sonnet 5.5's ($2, $10, $0.20): about 60–75% of it is the three
requests re-reading a 585k-token context, which no trim touches. The cards and batch JSON the
model writes come to about 5,600 output tokens, as estimated in #24; trimming them would save about
1,400 tokens, a cent or two. So the v1 figure is a formula, not a constant: **about 3 × the
session's context at the cache-read price, plus about 5,600 output tokens**, which is about $0.10–0.15
on Sonnet 5.5 after a short session (40k context) and about $0.47 after a long one. The design's
$0.28 and the $0.35 target are dropped.

## B-01

**The ledger runs under a TRUNCATE journal.** 2026-09-28 · forced by a platform constraint.

Cowork's connected folders refuse deletes, and both happenstance's WAL and SQLite's default
DELETE journal delete files beside the database: creating a ledger there failed with
`SQLITE_IOERR_DELETE` (2570). `Store::open` therefore opens its own connection under
`journal_mode=TRUNCATE` (`synchronous=FULL`) and copies happenstance's schema from a template
built on the local temporary disk, inside one `BEGIN IMMEDIATE`. A WAL ledger from 0.1.x converts
on first open; a stale `-wal` is renamed aside rather than deleted. Reproduced with a shim that
refuses `unlink`, and confirmed with strace on the computer's mount. Consequence: happenstance is
held at 0.3.2, and a happenstance schema change will need a migration path ([roadmap](../roadmap.md)).

## B-02

**The ledger location is settable.** 2026-09-28 · Claude's call, two-way.

First match wins: `--store`, `$WEIGHIN_STORE`, `store` in `.kb/_ledger/config.json`
(`init --ledger`), the git common dir, then `<folder>/.weighin/` outside git. The in-folder default
is safe on one computer; a synced folder used from several computers should point `store`
outside the synced tree.

## B-03

**Plugin binaries live in `libexec/`, not `bin/`.** 2026-09-28 · forced: claude.ai rejects hosted
plugins with a top-level `bin/`, because Claude Code adds it to PATH outside the admin approval
surface. Hooks and the skill call the launcher by path; `package-plugin.sh` refuses a `plugin/bin`.

## B-04

**Hooks write nothing in Cowork.** 2026-09-28 · Claude's call, two-way.

Under `CLAUDE_CODE_ENTRYPOINT=remote_cowork` the project folder is scratch, so `SessionStart`
prints the marker convention, `Stop` nudges on one-way markers, and nothing is written; the
sitting's sweep is the capture.

## B-05

**One binary version at a time.** 2026-09-28 · Claude's call, two-way.

The launcher prefers the first target listed unless a later one is a newer build, so a stale
native Windows build cannot shadow a fresh cross-build. The stale 0.1.1 msvc build was removed
from the local marketplace, since versions differ in journal mode.

## B-06

**The repository was seeded as one commit on top of GitHub's initial commit.** 2026-10-01 ·
Claude's call, two-way.

The pre-GitHub history stays only in the local branch `archive/pre-github`: it named a client's
Azure DevOps organisation in a test fixture and carried a stray telemetry file. GitHub's single
MIT `LICENSE` is replaced by `LICENSE-MIT` and `LICENSE-APACHE`, matching
`license = "MIT OR Apache-2.0"`. The release workflow packages with `--no-download` unless the
repository is public, so a release from this internal repository never points Cowork at a
download it cannot reach.

## B-07

**Every push to `main` and every pull request runs the gate in CI.** 2026-10-01 · Claude's call,
two-way.

Until now only a release tag ran tests and clippy, so a broken `main` surfaced at release time;
rustdoc under `-D warnings` was failing unnoticed. `.github/workflows/ci.yml` runs `cargo fmt
--check`, clippy with `-D warnings`, the tests and rustdoc with `-D warnings`, on Linux and
Windows, and the release workflow calls it instead of keeping its own copy. Dependabot opens
weekly update PRs for actions and Cargo minor and patch versions; Cargo majors (H-16) and
happenstance ([B-01](#b-01)) are held back.

## B-08

**A path is in the repository when its resolved form is under the resolved toplevel.**
2026-10-01 · Claude's call, two-way.

Hooks and agents spell paths as their surface does: 8.3 short names on Windows, symlinked temp
dirs on macOS, `..` components, and files not written yet. `Env::relative` resolves the nearest
existing directory, folds the rest (`..`, `.`) itself, and compares that with git's toplevel;
anchors keep their `:line:col`. When both sides resolve, that answer stands, so a file under an
in-repo directory symlink that points out of the repository is outside, as git treats it: it is
not scanned and nothing is annotated through the link. The file name itself is never resolved,
so an in-repo file symlink keeps its own name.

## B-09

**Phase 0 repository setup.** 2026-10-02 · Claude's call, two-way.

Release notes come from `CHANGELOG.md` (`scripts/changelog-section.sh`), and CI fails when the
current version has no entry, so a bump cannot ship without notes. A release tag must be on
`main`. CI adds macOS, a `plugin` job (stamp, versions, changelog, a packaged zip,
`claude plugin validate`) and `cargo deny` with a permissive licence list (MIT, Apache-2.0,
Unicode-3.0, Unlicense). Both crates are `publish = false`, which cargo-deny needs to accept the
workspace path dependency, and which matches D-0007. An `archive/*` pre-push guard lives in
`.githooks/`, opted into per clone. Claude Code cloud sessions run a synchronous SessionStart
hook that installs the toolchain and pre-builds the tests.

## B-10

**The ledger's schema version is `PRAGMA user_version`; newer ledgers are refused.** 2026-10-02 ·
Claude's call, two-way · closes the migration gap [B-01](#b-01) left.

happenstance keeps its own migration counter in `store_meta` and never touches `user_version`,
so the pragma is free for Weigh-In's own version: 1 is happenstance 0.3.2's schema under
TRUNCATE. A new ledger is stamped in the transaction that copies its schema. A 0.1.x or 0.2.x
ledger reads 0 and is migrated step by step in one `BEGIN IMMEDIATE` (0 → 1 changes nothing);
a ledger already current is not written to. A ledger stamped higher than the build knows is
refused with "upgrade weighin" and left untouched, rather than read by a build that may
misunderstand it. A happenstance schema change becomes one more step, which is the way past
0.3.2. Tests build a 0.1.x WAL ledger and a 0.2.2 TRUNCATE ledger in the test, the latter from
event payloads copied out of a ledger the 0.2.2 release wrote; older builds ignore the pragma,
so a 0.2.2 binary still opens a stamped ledger.

## B-11

**SessionStart tells the user when capture is off.** 2026-10-02 · Claude's call, two-way.

The message goes in `systemMessage`, which Claude Code shows to the user; plain SessionStart
stdout reaches only Claude's context, so the prime moves to `hookSpecificOutput.additionalContext`
whenever there is something to say, and stays plain stdout otherwise. Two cases:

- No binary. The `sh` launcher cannot parse the hook's JSON, so `hooks.json` passes the event
  name (`hook SessionStart`); the launcher drops it before running a binary, since the binary
  reads the event from stdin and an older one would refuse the extra argument. With no binary it
  prints a fixed JSON line (only `uname` output is interpolated, so nothing needs escaping).
- Hook errors. SessionStart counts `hook-errors.log` lines past the offset kept in
  `hook-errors.seen` beside it, reports the count and the last line, and moves the offset, so each
  line is reported once. A SessionStart that fails itself is logged first and reported in the same
  message, and Claude still gets the convention. The offset file is overwritten, never deleted
  ([B-01](#b-01)), and nothing is written when there is no log.

Under `CLAUDE_CODE_ENTRYPOINT=remote_cowork` neither is said ([B-04](#b-04)): Cowork captures
through the sitting's sweep, not hooks, so "capture is off" would be wrong there.

## B-12

**Cloud sessions on this repository get Weigh-In through the repository, not the plugin.**
2026-10-02 · Claude's call, two-way · implements [D-0010](#d-0010) for cloud sessions, as Ryan
chose; replaces enabling the plugin in `.claude/settings.json`, which cloud sessions ignore.

Claude Code's cloud sessions do not install plugins a repository enables (`enabledPlugins`,
`extraKnownMarketplaces`), but they do run its `.claude/settings.json` hooks and load its
`.claude/skills/`. So, in cloud sessions only (`CLAUDE_CODE_REMOTE=true`):
`.claude/hooks/session-start.sh` builds `weighin` into `plugin/libexec/<arch>-unknown-linux-gnu/`,
copies the plugin's skill into `.claude/skills/weigh-in/` (ignored by git) with
`${CLAUDE_PLUGIN_ROOT}` rewritten to the checkout's `plugin/`, and runs Weigh-In's SessionStart
itself, after the build, asking Claude Code to re-scan skills. A separate SessionStart hook would
run alongside the build and find no binary. Stop, SubagentStop and PostToolUse go through
`.claude/hooks/weigh-in.sh`, which runs the plugin's launcher. Both do nothing outside cloud
sessions, where the installed plugin captures, so nothing is captured twice.

## B-13

**The cloud SessionStart's lint step only warms the cache.** 2026-10-03 · Claude's call, two-way ·
fixes [B-12](#b-12) after temper's lint tables (#37) · closes
[#38](https://github.com/Wet-Ink-Corporation/weigh-in/issues/38).

temper's `[workspace.lints]` deny findings that `.claude/temper-baseline.json` records, so plain
`cargo clippy` fails on the legacy code, and under `set -e` that stopped `session-start.sh`
before it built `weighin`, installed the skill or ran Weigh-In's SessionStart: capture was off
in every cloud session, without a word. The hook now runs clippy with `--keep-going` and goes on
when it fails, saying the gate is `temper.mjs baseline --check`. Lints are judged by the gate,
never by session setup.

## B-14

**The `marketplace` branch is one commit built from the release zip, and has no ruleset.**
2026-10-03 · Claude's call, two-way, except the ruleset, which is Ryan's · implements
[D-0009](#d-0009) · closes [#12](https://github.com/Wet-Ink-Corporation/weigh-in/issues/12).

`scripts/marketplace-branch.sh` unpacks the zip the release uploads into `plugin/` beside
`main`'s `.claude-plugin/marketplace.json`, and commits it as the only commit of a fresh
repository, with the launcher and every binary set to mode 100755 whatever the zip kept. So
Claude Code installs exactly the plugin Cowork is given, skill stamped `V=<version>; S=none;`
while releases are internal ([D-0012](#d-0012)). `release.yml` builds and validates the tree
before publishing anything, then force-pushes it to `marketplace` after the release is created;
old binaries never stay in the history clones download, and Claude Code's update re-clones a
marketplace when it finds new commits, so the rewritten branch updates cleanly. CI builds the same tree from
its own zip on every pull request and checks the modes, the stamp and `claude plugin validate`.

Ryan chose no ruleset for the branch: a ruleset cannot name the workflow's `GITHUB_TOKEN` as a
bypass actor, so guarding it would need a GitHub App or a deploy key, and the `main` ruleset
already covers only `main`. People with write access could push it; the next release replaces it.

## B-15

**A plugin package holds only the launchers and one binary per target.** 2026-10-03 · Claude's
call, two-way · closes [#43](https://github.com/Wet-Ink-Corporation/weigh-in/issues/43).

Rerunning the `release` job of v0.4.0 downloaded every artifact of the run into
`plugin/libexec/`, including the `release-assets` artifact the failed first attempt had
uploaded, so the zip and the `marketplace` branch carried a nested copy of the release (32 MB
instead of about 13). The job now downloads only artifacts named like a target (`*-*-*`), and
`package-plugin.sh` refuses a `libexec/` holding anything but `weighin`, `weighin.cmd` and
`<target>/weighin[.exe]`, so a stray file fails the release instead of shipping. CI checks the
refusal on every pull request. The first attempt failed because a GitHub release for the tag
already existed; the README now says to push only the tag.

## B-16

**Hooks start the launcher directly (exec form), with the Windows build at `libexec/weighin.exe`.**
2026-10-03 · Claude's call, two-way · implements [D-0015](#d-0015) · amends [B-11](#b-11) for
Windows.

Each hook in `hooks.json` is `{"command": "${CLAUDE_PLUGIN_ROOT}/libexec/weighin", "args":
["hook", …]}`. With `args`, Claude Code (2.1.139 and later) spawns the command with no shell on
every platform, Git Bash or not. A spike under Bun, Claude Code's runtime
([#56](https://github.com/Wet-Ink-Corporation/weigh-in/pull/56)), settled the rest:

- On Linux and macOS the kernel runs the `sh` launcher by its shebang, so it must be executable.
  The `marketplace` branch commits it 100755 ([B-14](#b-14)); with the bit cleared the hook fails
  with `EACCES`.
- On Windows the same extensionless path resolves to `libexec/weighin.exe`, with forward or back
  slashes and with no Git on `PATH`; the hook JSON reaches stdin. A `.cmd` cannot be spawned this
  way (`EINVAL`), so `weighin.cmd` cannot stand in.
- `package-plugin.sh` copies the Windows build (msvc, else gnu) to `libexec/weighin.exe` in the
  package, and the local build scripts do the same, so the newest build is there ([B-05](#b-05)).
  The release ships the msvc build twice, once per path; the per-target copy stays for the `sh`
  launcher the skill uses.
- Nothing strips the event name on Windows, so `weighin hook` takes it as an optional argument
  and still reads the event from stdin. Hooks of 0.5.0 need a 0.5.0 binary, which the same
  package ships.

What it costs: on Windows a missing `weighin.exe` is a hook error (`ENOENT`), not [B-11](#b-11)'s
"capture is off" message, which still covers Linux and macOS. Claude Code older than 2.1.139 has
no exec form. Unverified: whether plugins synced from the uploaded zip (Cowork, a claude.ai
upload) keep the launcher's executable bit, which `sh launcher` never needed; if they don't, hooks
there fail with `EACCES`, and Cowork loses its SessionStart convention. To check in Cowork before
0.5.0 is tagged. Rejected: a second hook entry with
`"shell": "powershell"`, which fails on macOS and Linux without `pwsh` and runs twice where both
shells exist; one shell-form string for both shells, which neither parses; `commandWindows`, which
Claude Code ignores.


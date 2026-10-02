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

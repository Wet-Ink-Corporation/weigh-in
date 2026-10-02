# Plan: from 0.2.2 to v1 in active use

Evaluation of the repository and its source control on 2026-10-02, and the plan that follows from
it. It starts with getting the repository organised for ongoing development and ends with v1
live. It adds no decisions: the calls it needs are listed under [Decisions needed](#decisions-needed)
and get recorded in `docs/decisions/` when they are taken.

## Where Weigh-In stands

**Built and working (slice 1).** Capture by hooks at zero model tokens, the in-chat sitting
(`/weigh-in`), the five filters and card guards in the CLI, `.kb` atoms with source annotation and
commit trailers, the Artifact board for own repos, the Cowork route through the computer's shell,
and the TRUNCATE-journal ledger that survives Cowork's refused deletes. About 7,600 lines of Rust, tests included,
over two crates (`weighin-core` with no I/O, `weighin` the CLI), an 80-line skill and a 41-line
launcher.

**Gate is green.** Checked on this branch at `c54139a`: `cargo test --workspace` passes 57 tests
(31 core, 18 store/git/paths, 8 end to end), and `cargo fmt --check`, clippy with `-D warnings`,
rustdoc with `-D warnings` and `claude plugin validate .` are all clean. CI (`ci.yml`) has passed
on every run on `main` since it was added, on Linux and Windows.

**Released once.** `v0.2.2` is tagged and published (Linux musl, Windows msvc, macOS arm64,
the zip and `SHA256SUMS`). The first release run failed on a slow Windows runner and passed on
re-run; `ae3ccb0` widened the launcher test's deadline to stop it recurring.

**Decisions are well kept.** Seven board decisions (MADR, one file per `wi-` id), D-0001 to
D-0007 and B-01 to B-08 in `build-decisions.md`, each with provenance and what it supersedes.
`docs/roadmap.md` lists the open verifications (V1, V2, V4–V7), known gaps and slice 2.

**Measured in use, but only in trials.** Live Claude Code sittings on Sonnet cost $0.42–0.57
against a $0.28 design estimate. No repository records a sustained run of real sittings, and
this repository does not use Weigh-In on itself: it has no `.kb/`, and its decisions are kept
by hand.

### Source control and repository setup

| Area | Now | Gap |
|---|---|---|
| History | 13 commits on `main`, linear, clear messages; pre-GitHub history kept only in a local `archive/pre-github` branch (B-06) | That branch names a client and must never be pushed; nothing enforces that beyond memory |
| Branching | Ryan's commits go straight to `main` (`ae3ccb0`, `60c5620`, `c54139a`); only Dependabot opens PRs | No ruleset requiring a PR and green CI on `main`; CI catches a break only after it has landed |
| Tags and releases | `v*` tag triggers `release.yml`, which checks the versions agree with the tag | Any `v*` tag publishes, from any commit; nothing checks the tagged commit is on `main`; no tag protection |
| CI coverage | fmt, clippy, tests, rustdoc on Linux and Windows | macOS is built for release but never tested, though B-08's symlinked-temp-dir case is macOS's; `claude plugin validate` and `package-plugin.sh` (stamp check, no `plugin/bin`) run only by hand |
| Dependencies | Dependabot weekly for actions and Cargo; majors and happenstance held back | No `cargo audit` / `cargo deny`; happenstance pin has no exit path yet (B-01) |
| Work tracking | `docs/roadmap.md` only; no issues, labels or milestones on GitHub | Open items cannot be assigned, linked from PRs or closed by a commit |
| Contributor scaffolding | `CLAUDE.md` holds the gate and the hard-won rules | No PR template, `CHANGELOG.md` or CODEOWNERS; release notes are a fixed sentence |
| Repository scope | `docs/research/reports/Shipping native binaries in plugins.md` and its notes (`60c5620`, `c54139a`) are mostly about temper | Research for another project lives here; temper's own repository is its natural home, with a link back |
| Cloud sessions | Toolchain is pinned (`rust-toolchain.toml`, 1.98.1) | No SessionStart hook for Claude Code cloud sessions, so a fresh session depends on the image having the toolchain |

### What stands between 0.2.2 and active use

1. **A git or marketplace install has no binaries.** `plugin/libexec/*/` is ignored, so
   `/plugin install weigh@weigh-in` from the repository gets launchers and no `weighin`. The
   launcher then exits 0 for every hook (`plugin/libexec/weighin`, last lines), so capture is
   silently off. Today it works only after `scripts/build-plugin-bins.*` on each machine, or from
   the uploaded zip.
2. **Failures are silent.** Hook errors go to `<state dir>/hook-errors.log` and the hook exits 0
   (`crates/weighin/src/hooks.rs`). That is right for a fail-soft ledger, but nothing ever tells
   the user that capture has been off. The native-binaries research names this as the one thing
   temper must not copy.
3. **Cowork download is dormant** while releases are internal (D-0003, D-0007): Cowork works only
   with a connected folder holding this repository.
4. **Six surfaces are unverified** (V1, V2, V4–V7), including the Windows binary from an installed
   plugin (V7) and the Cowork session id (V1).
5. **No upgrade guarantee.** The ledger copies happenstance 0.3.2's schema (B-01) with no schema
   version or migration step; a v1 promise to keep a user's ledger needs one, and a test that opens
   a 0.2.x ledger.
6. **Single-user defaults.** The owner is `ryan` unless `init --owner` sets it
   (`crates/weighin/src/env.rs:93`), so records say `human:ryan` for anyone else.
7. **Small launcher drift.** `weighin.cmd` prefers the msvc build whatever its age, while the `sh`
   launcher takes the newer build (B-05); the release ships only msvc, so this bites only local builds.
8. **Sitting cost** is roughly double the estimate. Not a blocker, but v1 should either set the
   measured figure as the target or trim the batch and card output.

## Decisions needed

These are Ryan's calls, and the plan's later phases depend on them. Each gets an entry in
`build-decisions.md` (or a board sitting) when taken. The recommendation is given in "A unless X" form.

| # | Question | Options | Recommendation | Blocks |
|---|---|---|---|---|
| P-1 | What does "v1 live" mean? | (a) Ryan uses it daily on his own machines and in Cowork; (b) also colleagues at Wet Ink; (c) a public plugin anyone can install | (a), with (b) possible without rework: owner from git config, install without a toolchain. (c) is a later release | Phase 4 exit criteria |
| P-2 | Where do installable binaries come from outside the zip? | (a) the release workflow publishes a built plugin (binaries included) to a `marketplace` branch or a release-only repo, and `marketplace.json` points there; (b) SessionStart downloads the pinned binary into `${CLAUDE_PLUGIN_DATA}`; (c) keep building from source | (a): the binary is on disk before any hook fires, which the native-binaries research found is the design that holds up; (b) only as a fallback. (c) unless no one but Ryan installs it | Phase 2 |
| P-3 | Should release assets be public? | (a) make this repository public; (b) a public release-only repository; (c) stay internal and keep Cowork on the connected folder | (b): turns on the Cowork download (D-0003) and P-2(a) without publishing the research, which names IQBG and client-repo policy. (a) after a content audit, if Ryan wants the source open | Phase 2, V1 in Cowork |
| P-4 | Does this repository use Weigh-In on itself? | (a) yes: `init --class own`, `.kb/` committed, new decisions through `/weigh-in`; (b) keep the hand-kept records | (a): it is the cheapest way to get the sustained use v1 needs; the hand-kept records stay as history | Phase 3 |
| P-5 | Where does open work live? | (a) GitHub issues with a `v1.0` milestone, `roadmap.md` summarising and linking them; (b) `roadmap.md` only | (a): issues can be linked from PRs and closed by commits | Phase 0 |
| P-6 | Is slice 2 in v1? | Grounder and stability check, `serve --wait`, suppression, JIT surfacing, drills, door classifier, kb-ingest | None of it: v1 is slice 1 made dependable. Slice 2 starts after 1.0 | Scope |

## The plan

Five phases. Sizes are rough working days for one person with an agent; phases 0 and 1 can
overlap. Each phase ends at a version so progress is visible in releases.

### Phase 0 — Organise the repository (about 1 day) → no release

The aim is that every later change arrives by PR, is gated, and is tracked.

1. **Ruleset on `main`** (GitHub settings): require a pull request, require the `ci` checks
   (`check (ubuntu-latest)`, `check (windows-latest)`) to pass, block force pushes and deletion,
   squash or rebase merges only so history stays linear. Ryan may bypass, for emergencies only.
2. **Tag ruleset for `v*`**: only Ryan creates them. Add a step to `release.yml` that fails unless
   the tagged commit is an ancestor of `origin/main`.
3. **Work tracking (P-5)**: labels `verify`, `gap`, `release`, `decision`, `slice-2`, `deps`;
   a `v1.0` milestone; one issue per V-item, known gap and phase task below. `roadmap.md`
   becomes a short summary that links them.
4. **Contributor scaffolding**: `.github/pull_request_template.md` (what changed, the gate run,
   decisions touched, version bump yes/no); `CHANGELOG.md` starting at 0.2.2, written from the
   commit messages; `.github/CODEOWNERS` with Ryan as owner. Keep `CLAUDE.md` as the one
   contributor guide rather than adding `CONTRIBUTING.md`.
5. **CI additions**: a macOS leg in `ci.yml`; a `plugin` job that runs `claude plugin validate .`
   and `scripts/package-plugin.sh --no-download` against a Linux build (stamp count, no
   `plugin/bin`, zip produced); `cargo deny check` (advisories, licences) as its own job.
6. **Release notes from the changelog**: `release.yml` takes the `## <version>` section of
   `CHANGELOG.md` as the release body and fails when it is missing.
7. **Housekeeping**: move the temper research (`docs/research/reports/Shipping native binaries in
   plugins.md` and its notes) to temper's repository, keeping a one-paragraph summary of the
   Weigh-In findings here and linking it from `docs/README.md`. Write a pre-push hook, or a note
   in `CLAUDE.md`, that stops `archive/pre-github` from ever being pushed.
8. **Cloud sessions**: a SessionStart hook in `.claude/settings.json` that installs the pinned
   toolchain and warms the cargo cache, so any Claude Code cloud session can run the gate.

Done when: a test PR cannot merge red, an unprotected tag cannot release, every roadmap item is an
issue in the `v1.0` milestone, and CI covers macOS and the plugin package.

### Phase 1 — Dogfood and upgrade safety (about 2–3 days) → 0.3.0

1. **Use Weigh-In here (P-4)**: `weighin init --class own` in this repository, commit `.kb/`, and
   run `/weigh-in` at the end of every working session on it from now on. New decisions get atoms;
   `docs/decisions/` stays as the record of everything before.
2. **Ledger schema version**: stamp `PRAGMA user_version` in `Store::open`, add a migration step
   (a no-op for the first version), and add fixture tests that open 0.1.x WAL and 0.2.2 TRUNCATE
   ledgers and read them. This gives happenstance a way past 0.3.2 (B-01).
3. **Owner from the user**: default `owner` to `git config user.email` (or `user.name`), falling
   back to `$USER`; `init --owner` still overrides. Update the tests that assume `ryan`.
4. **Make silent failure visible**: when the launcher finds no binary, or `hook-errors.log` has new
   lines since the last session, SessionStart prints one line saying capture is off and why.
   Hooks still exit 0.
5. **Launcher parity**: `weighin.cmd` picks the newer of msvc and gnu, as the `sh` launcher does (B-05).
6. Bump to 0.3.0 and release.

### Phase 2 — Distribution (about 2–3 days, after P-2 and P-3) → 0.4.0

1. **Public release assets (P-3)**: create the release-only repository (or make this one public
   after a content audit), and have `release.yml` publish there.
2. **Installable marketplace (P-2)**: the release workflow writes the built plugin, binaries
   included and the skill stamped, to the published marketplace, so `/plugin marketplace add`
   and `/plugin install weigh@weigh-in` give a working plugin with no toolchain. The repository's
   own `marketplace.json` stays for development (`--plugin-dir` or local builds).
3. **Turn on the Cowork download**: package without `--no-download` once assets are public;
   check on a Cowork chat that the binary downloads, matches the pinned SHA-256 and runs.
4. **Optional fallback**: SessionStart downloads the pinned binary into `${CLAUDE_PLUGIN_DATA}`
   when `libexec/` has none. Do this only if P-2 chose (b) as well; its persistence in Cowork is
   still unverified.
5. **Install guide for users**, separate from the developer build steps in `README.md`: install,
   update, uninstall, where the ledger is, how to read `hook-errors.log`.
6. Bump to 0.4.0 and release.

### Phase 3 — Verify every surface and run it for real (about 2 weeks elapsed, little effort) → 0.9.x

1. **Close V1–V7**, each with a dated note in its issue and in `roadmap.md`:
   V1 Cowork session id; V2 ArtifactData from Cowork; V4 editor links from the board; V5 `db` store
   quotas (push a large ledger and note where it fails); V6 the desktop Code tab and the Cowork
   question card; V7 the Windows binary from an installed plugin. Add the Windows-without-Git-Bash
   case, since hooks run under `sh`: either confirm Claude Code on Windows always provides it, or
   ship a `cmd`/PowerShell hook form.
2. **Sustained use**: at least two weeks of real sittings across this repository and two of
   Ryan's own repos, plus one Cowork folder. Each week, record `weighin stats` (action rate,
   not-useful rate, overrides, time to decide) and the sitting cost in this plan's issue.
3. **Sitting cost**: either trim the batch JSON and card output until a sitting is under about
   $0.35, or record the measured cost as the v1 figure in a decision.
4. Fix what use finds, releasing 0.9.x release candidates through the same workflow.

Exit criteria (proposed for P-1(a)):

- Every V-item is closed or has a recorded decision to defer it past 1.0.
- No ledger corruption and no unexplained `hook-errors.log` lines over the two weeks.
- A not-useful rate low enough that Ryan keeps running sittings (target: under 30% of cards dismissed `not-useful`).
- A fresh machine (Linux, Windows, macOS) goes from nothing to a working `/weigh-in` with the
  install guide alone, without Rust.
- A 0.2.2 ledger opens and reads correctly under the candidate.

### Phase 4 — v1.0 live (about 1 day) → 1.0.0

1. Freeze for review: run `/code-review` over the changes since 0.2.2 and fix what it finds.
2. Update `README.md` ("Measured in this build"), `roadmap.md` (slice 2 becomes the next
   milestone), `CHANGELOG.md`, and add a dated amendment to `docs/research/report.md`
   summarising what v1 shipped against the design.
3. Bump both versions to 1.0.0, merge, tag `v1.0.0`; the release workflow publishes the release
   and the marketplace.
4. Install 1.0.0 from the published marketplace on each of Ryan's machines and upload the zip to
   claude.ai for Cowork; remove any local-build installs so only one version runs (B-05).
5. Record "v1 live" in `build-decisions.md` with the exit-criteria results, and open the
   `v1.1` / slice 2 milestone.

## After v1

Slice 2 as in `roadmap.md`: the Sonnet grounder and stability check, `weighin serve --wait` for
client repos, per-category suppression, just-in-time surfacing, reliance drills (wi-2b90f3), the
door classifier for unattended runs (wi-4d8b6a, which also settles the open model-key question),
the board from Cowork, and the move into redkiln-rs with kb-ingest of the intake keys
(wi-e15c77, wi-b4c1d8).

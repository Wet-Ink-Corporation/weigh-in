# Plan: from 0.2.2 to v1 in active use

Evaluation of the repository and its source control on 2026-10-02, and the plan that follows from
it. It starts with getting the repository organised for ongoing development and ends with v1
live. The calls it needs are listed under [Decisions](#decisions); those taken are recorded in
`docs/decisions/build-decisions.md`.

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

## Decisions

Ryan answered all six on 2026-10-02; they are recorded as D-0008 to D-0013 in
[`build-decisions.md`](decisions/build-decisions.md).

| # | Question | Answer | Effect on the plan |
|---|---|---|---|
| P-1 | What does "v1 live" mean? | **Daily use by Ryan** (D-0008) | Exit criteria are Ryan's own machines and Cowork; no install path for other people, and the `ryan` owner default stops blocking |
| P-2 | Where do installable binaries come from outside the zip? | **A built marketplace in this internal repository** (D-0009) | The release workflow publishes the built plugin to a `marketplace` branch here |
| P-3 | Should release assets be public? | **Stay internal for v1** (D-0012) | One repository; Cowork through a connected checkout |
| P-4 | Does this repository use Weigh-In on itself? | **Yes** (D-0010) | Phase 1, step 1 |
| P-5 | Where does open work live? | **GitHub Issues** (D-0011) | Phase 0, step 3 |
| P-6 | Is any slice 2 work in v1? | **None, unless not-useful exceeds 10%** (D-0013) | Per-category suppression is conditional in Phase 3 |

**P-3: public assets, and whether cleaning up the research avoids a second repository.** The
second repository was only ever a way to make release assets public. Nothing else needed it. So
cleaning up the research doesn't change how many repositories there are: it is what would let
*this* one go public, and one repository is the end state either way. With v1 for Ryan alone,
nothing needs to be public:

- Claude Code installs from a private GitHub marketplace with the git credentials already on
  the machine (`gh auth setup-git` or SSH). Background auto-update uses the same credential
  helper and fails quietly without one; a bare `GITHUB_TOKEN` is not enough on its own
  ([docs](https://code.claude.com/docs/en/plugins/install.md#add-a-private-marketplace)).
- Cowork takes the plugin as an uploaded zip, which Ryan downloads from the internal release.
  On the computer's shell, the skill finds the Linux binary in a connected folder
  (`$HOME/mnt/*/libexec/x86_64-unknown-linux-musl/weighin`). A local checkout of the
  `marketplace` branch, connected once, provides that binary with no build and no download.

Going public later means cleaning up the research (it names IQBG and client-repo policy, in
`docs/research/` and `wi-7c21e4`), then making this repository public and packaging without
`--no-download`. The Cowork download then works with no more changes. That is a post-v1 step.

**P-6: what slice 2 is.** The design ([report](research/report.md#first-slice-and-what-follows-lower-bound-estimates))
had three slices. Slice 1 has been built, plus slice 2's board. What remains, with the design's
lower-bound estimates:

| Item | What it does | Estimate |
|---|---|---|
| Sonnet grounder and stability check | A second model writes or checks each card's grounding, and the recommendation is re-judged three times with the options shuffled; an unstable one is shown options-first | 2.5 h |
| `weighin serve --wait` | A local review page for client repos, where the Artifact board is off ([wi-7c21e4](decisions/wi-7c21e4.md)) | 2.5 h |
| Metrics and per-category suppression | Stop raising item kinds that keep being dismissed `not-useful` (`weighin stats` already measures this) | 1.5 h |
| Just-in-time surfacing | A hook shows a past decision when an edit touches the code it is anchored to | 1 h |
| Board from Cowork | The Artifact board in Cowork, staging state through the device shell | not estimated |
| Reliance drills | 1 in 30 two-way cards recommends a weaker option, to check you are not rubber-stamping ([wi-2b90f3](decisions/wi-2b90f3.md)) | not estimated |
| Door classifier for unattended runs | An independent model judges one-way or two-way before an unattended run defaults anything ([wi-4d8b6a](decisions/wi-4d8b6a.md)) | not estimated |
| Slice 3: Redkiln-native | `decision` item type in redkiln-rs, kb-ingest of intake keys, AFK gating, a "Needs you" column, an SDK pause loop ([wi-e15c77](decisions/wi-e15c77.md)) | 15 h |

Decided (D-0013): none of it is in v1, with one conditional exception. If Phase 3's not-useful rate
misses its 10% bar, per-category suppression comes into v1, because it is the design's fix for
exactly that miss. Everything else serves client repos, unattended runs or Redkiln, and v1 needs
none of those for Ryan's daily use.

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
3. **Make silent failure visible**: when the launcher finds no binary, or `hook-errors.log` has new
   lines since the last session, SessionStart prints one line saying capture is off and why.
   Hooks still exit 0.
4. **Launcher parity**: `weighin.cmd` picks the newer of msvc and gnu, as the `sh` launcher does (B-05).
5. Bump to 0.3.0 and release.

### Phase 2 — Distribution from the internal repository (about 2 days) → 0.4.0

1. **Marketplace branch (D-0009)**: after publishing the release, `release.yml` force-pushes a
   single-commit `marketplace` branch. It holds `.claude-plugin/marketplace.json` and `plugin/`,
   with the Linux musl, Windows msvc and macOS arm64 binaries in `libexec/<target>/`, committed
   executable. Replacing the commit each release keeps old binaries out of the history that
   clones download. The workflow needs `contents: write`, which it already has. Exclude the
   branch from the `main` ruleset, and allow only the workflow to push it.
2. **Install from it**: `/plugin marketplace add Wet-Ink-Corporation/weigh-in#marketplace`, then
   `/plugin install weigh@weigh-in`, on each of Ryan's machines with `gh auth setup-git` done.
   Check that hooks fire with no toolchain installed, and that background update picks up the
   next release. The docs don't say whether a plugin keeps its executable bit in the cache; the
   launcher already sets it (`chmod +x`) when it is missing.
3. **Cowork binary**: keep a local checkout of the `marketplace` branch and connect it once to
   Cowork, so the skill's connected-folder fallback finds the Linux binary. Upload the release
   zip under Customize > Plugins. Releases stay `--no-download` (D-0012).
4. **Developer route unchanged**: `main`'s `marketplace.json` and `scripts/build-plugin-bins.*`
   stay for `--plugin-dir` and local builds.
5. **Install guide**, separate from the developer build steps in `README.md`: install, update,
   uninstall, where the ledger is, how to read `hook-errors.log`.
6. Bump to 0.4.0 and release.

### Phase 3 — Verify every surface and run it for real (about 2 weeks elapsed, little effort) → 0.9.x

1. **Close V1–V7**, each with a dated note in its issue and in `roadmap.md`:
   V1 Cowork session id; V2 ArtifactData from Cowork; V4 editor links from the board; V5 `db` store
   quotas (push a large ledger and note where it fails); V6 the desktop Code tab and the Cowork
   question card; V7 the Windows binary from an installed plugin. Add the Windows-without-Git-Bash
   case, since hooks run under `sh`: either confirm Claude Code on Windows always provides it, or
   ship a `cmd`/PowerShell hook form.
2. **Eval seed**: label 20 items from five real sessions (the design's slice 1 eval set, not yet
   built), so marker capture precision can be measured.
3. **Sustained use**: at least two weeks of real sittings across this repository and two of
   Ryan's own repos, plus one Cowork folder. Each week, record `weighin stats` (action rate,
   not-useful rate, overrides, time to decide) and the sitting cost in this plan's issue.
4. **Sitting cost**: either trim the batch JSON and card output until a sitting is under about
   $0.35, or record the measured cost as the v1 figure in a decision.
5. Fix what use finds, releasing 0.9.x release candidates through the same workflow.

Exit criteria. These are the design's slice 1 exit criteria (two weeks or 30 presented items,
whichever comes later), the same targets `weighin stats` prints, plus the release checks:

- Action rate at least 60%; not-useful dismissals at most 10%. If not-useful is above 10%,
  per-category suppression comes into v1 (P-6).
- Marker capture precision at least 80% on the eval seed.
- No "pending" item is ever reported as "done", and at least one sitting correctly returns
  "no decisions need you".
- Every V-item is closed or has a recorded decision to defer it past 1.0.
- No ledger corruption, and no unexplained `hook-errors.log` lines.
- Each of Ryan's machines goes from nothing to a working `/weigh-in` through the marketplace
  branch, without Rust; Cowork works through the connected checkout.
- A 0.2.2 ledger opens and reads correctly under the candidate.

### Phase 4 — v1.0 live (about 1 day) → 1.0.0

1. Freeze for review: run `/code-review` over the changes since 0.2.2 and fix what it finds.
2. Update `README.md` ("Measured in this build"), `roadmap.md` (slice 2 becomes the next
   milestone), `CHANGELOG.md`, and add a dated amendment to `docs/research/report.md`
   summarising what v1 shipped against the design.
3. Bump both versions to 1.0.0, merge, tag `v1.0.0`; the release workflow publishes the release
   and the marketplace.
4. Update each of Ryan's machines from the `marketplace` branch, refresh the connected Cowork
   checkout and upload the zip to claude.ai; remove any local-build installs so only one version runs (B-05).
5. Record "v1 live" in `build-decisions.md` with the exit-criteria results, and open the
   `v1.1` / slice 2 milestone.

## After v1

Slice 2 as in `roadmap.md`: the Sonnet grounder and stability check, `weighin serve --wait` for
client repos, per-category suppression, just-in-time surfacing, reliance drills (wi-2b90f3), the
door classifier for unattended runs (wi-4d8b6a, which also settles the open model-key question),
the board from Cowork, the owner default taken from git config (needed once anyone but Ryan
uses it), going public (D-0012), and the move into redkiln-rs with kb-ingest of the intake keys
(wi-e15c77, wi-b4c1d8).

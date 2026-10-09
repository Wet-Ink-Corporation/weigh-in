# Roadmap and open items

State at 0.5.4 (2026-10-06, current on 2026-10-07). Slice 1 is built: capture, the in-chat
sitting, decision records, the Cowork route through the computer's shell, and the local
marketplace. Phases 1 and 2 of the v1 plan are done. Phase 3 is under way: 0.5.0 runs the hooks
with no shell, so capture works on Windows without Git Bash ([D-0015](decisions/build-decisions.md#d-0015),
[B-16](decisions/build-decisions.md#b-16)); 0.5.1 caps the board's defaults below the Artifact
store's document limit and stops read-only commands from creating a ledger; 0.5.2 tells agents to
write one block per call when they list calls for the user ([#68](https://github.com/Wet-Ink-Corporation/weigh-in/issues/68), wi-357345);
0.5.3 publishes each release to `weigh-in-plugin` for claude.ai's organization sync
([B-19](decisions/build-decisions.md#b-19)); 0.5.4 has the launcher name a copy with no binaries
([B-18](decisions/build-decisions.md#b-18)). The sitting cost is recorded as a formula
([D-0016](decisions/build-decisions.md#d-0016)). V4 and V5 are closed, V2 is deferred to slice 2
([B-17](decisions/build-decisions.md#b-17)), and V6 passed in the desktop Code tab. Phase 4 has
started: the review since 0.2.2 and drafts of the closing documents are on
[#26](https://github.com/Wet-Ink-Corporation/weigh-in/issues/26). 1.0.0 waits on the exit
criteria below. After v1 the next milestone is slice 2
([#27](https://github.com/Wet-Ink-Corporation/weigh-in/issues/27)). The path is in
[`v1-plan.md`](v1-plan.md).

v1 exit criteria ([v1 plan](v1-plan.md), Phase 3), on 2026-10-07:

| Criterion | Status |
|---|---|
| Action rate at least 60%, not-useful at most 10% | **Pending:** no week of stats on [#23](https://github.com/Wet-Ink-Corporation/weigh-in/issues/23) yet (week 1 is due on or after 2026-10-12); [#25](https://github.com/Wet-Ink-Corporation/weigh-in/issues/25) waits on it |
| Marker capture precision at least 80% | **Met on draft labels:** 12 of 12 on the seed (70 items, s12 added on 2026-10-07), so the bar is in force (wi-a9df17); **pending** Ryan's label review ([#22](https://github.com/Wet-Ink-Corporation/weigh-in/issues/22)) |
| No "pending" reported as "done"; one sitting returns "no decisions need you" | **Pending:** measured over the two weeks of use ([#23](https://github.com/Wet-Ink-Corporation/weigh-in/issues/23)) |
| Every V-item closed or deferred by a decision | **Pending:** a record that V3 is moot under [wi-6e2a47](decisions/wi-6e2a47.md) (one store per repository); V1 and the Cowork half of V6 ([#16](https://github.com/Wet-Ink-Corporation/weigh-in/issues/16), [#20](https://github.com/Wet-Ink-Corporation/weigh-in/issues/20)); a marker captured on Windows without Git Bash ([#21](https://github.com/Wet-Ink-Corporation/weigh-in/issues/21)) |
| No ledger corruption, no unexplained `hook-errors.log` lines | **Pending:** over the two weeks of use |
| Each machine installs without Rust; Cowork through the connected checkout | The `marketplace` branch: run on Windows and in Cowork ([#13](https://github.com/Wet-Ink-Corporation/weigh-in/issues/13)); macOS and Linux unverified. **Pending:** the organization-synced `weigh-in-plugin` route, Ryan's Windows check ([#76](https://github.com/Wet-Ink-Corporation/weigh-in/issues/76)) |
| A 0.2.2 ledger opens under the candidate | Fixture tests pass on `main` (B-10); to recheck on the 1.0.0 candidate |

Open work is tracked as GitHub issues ([D-0011](decisions/build-decisions.md#d-0011)), in the order of
the [v1 plan](v1-plan.md). This page summarises them; the issue is where status is kept.

| Phase | Issues |
|---|---|
| 0: organise the repository | [#5](https://github.com/Wet-Ink-Corporation/weigh-in/issues/5) rulesets on `main` and `v*` tags · [#28](https://github.com/Wet-Ink-Corporation/weigh-in/issues/28) scaffolding, CI coverage, issue tracking · [#6](https://github.com/Wet-Ink-Corporation/weigh-in/issues/6) move the temper research |
| 1: use it here, upgrade safety (0.3.0) · **done** | [#7](https://github.com/Wet-Ink-Corporation/weigh-in/issues/7) use Weigh-In on this repository · [#8](https://github.com/Wet-Ink-Corporation/weigh-in/issues/8) ledger schema version · [#9](https://github.com/Wet-Ink-Corporation/weigh-in/issues/9) SessionStart says when capture is off · [#10](https://github.com/Wet-Ink-Corporation/weigh-in/issues/10) `weighin.cmd` picks the newer build · [#11](https://github.com/Wet-Ink-Corporation/weigh-in/issues/11) release |
| 2: distribution (0.4.x) · **done** | [#12](https://github.com/Wet-Ink-Corporation/weigh-in/issues/12) `marketplace` branch from `release.yml` · [#13](https://github.com/Wet-Ink-Corporation/weigh-in/issues/13) install from it; Cowork through a connected checkout · [#14](https://github.com/Wet-Ink-Corporation/weigh-in/issues/14) install guide · [#15](https://github.com/Wet-Ink-Corporation/weigh-in/issues/15) release · fixes from the installs: [#43](https://github.com/Wet-Ink-Corporation/weigh-in/issues/43) release rerun, [#46](https://github.com/Wet-Ink-Corporation/weigh-in/issues/46) auto-update, [#48](https://github.com/Wet-Ink-Corporation/weigh-in/issues/48) 0.4.1 |
| 3: verify and use (0.9.x) · **under way** | [#16](https://github.com/Wet-Ink-Corporation/weigh-in/issues/16) V1 · [#17](https://github.com/Wet-Ink-Corporation/weigh-in/issues/17) V2 · [#18](https://github.com/Wet-Ink-Corporation/weigh-in/issues/18) V4 · [#19](https://github.com/Wet-Ink-Corporation/weigh-in/issues/19) V5 · [#20](https://github.com/Wet-Ink-Corporation/weigh-in/issues/20) V6 · [#21](https://github.com/Wet-Ink-Corporation/weigh-in/issues/21) V7 and hooks without Git Bash · [#22](https://github.com/Wet-Ink-Corporation/weigh-in/issues/22) eval seed · [#23](https://github.com/Wet-Ink-Corporation/weigh-in/issues/23) two weeks of daily use · [#24](https://github.com/Wet-Ink-Corporation/weigh-in/issues/24) sitting cost (**done**, D-0016) · [#25](https://github.com/Wet-Ink-Corporation/weigh-in/issues/25) suppression, only if not-useful exceeds 10% |
| 4: v1.0 · **started** | [#26](https://github.com/Wet-Ink-Corporation/weigh-in/issues/26) release 1.0.0 and record v1 live |
| After v1: the next milestone | [#27](https://github.com/Wet-Ink-Corporation/weigh-in/issues/27) slice 2 and later |

## Not yet verified

| Id | What | Issue |
|---|---|---|
| V1 | Session identity in Cowork: whether `${CLAUDE_SESSION_ID}` is substituted in a synced skill | [#16](https://github.com/Wet-Ink-Corporation/weigh-in/issues/16) |
| V2 | ArtifactData reads and writes from Cowork. **Deferred to slice 2** on 2026-10-05 ([B-17](decisions/build-decisions.md#b-17)): the board does not run from Cowork in v1 | [#17](https://github.com/Wet-Ink-Corporation/weigh-in/issues/17) |
| V4 | Editor links (`vscode://file/…`) opened from inside an Artifact. **Closed 2026-10-05: not supported.** On Windows, `vscode://` and `claude-cli://` links do nothing with `target=_top` and blank the frame without it; `https://` opens in the browser. The board keeps selectable `path:line` text | [#18](https://github.com/Wet-Ink-Corporation/weigh-in/issues/18) |
| V5 | Quotas of the board's `db` store. One document holds at most 256 KiB, which about 700 defaults overflowed; 0.5.1 caps the board at 200 ([#61](https://github.com/Wet-Ink-Corporation/weigh-in/pull/61)). **Run 2026-10-05:** the limit is exactly 262,144 bytes; a 1,000-default ledger exports 59,678 bytes (200 shown) and round-trips through `ArtifactData`; the store allows 25,000 documents per artifact. **Closed 2026-10-05**: Ryan saw the probe render the capped state | [#19](https://github.com/Wet-Ink-Corporation/weigh-in/issues/19) |
| V6 | Previews in the desktop Code tab and the Cowork question card. **Code tab, 2026-10-05:** a sitting's card worked, and the selected option's preview came back with the answer. The Cowork card is checked during V1's run | [#20](https://github.com/Wet-Ink-Corporation/weigh-in/issues/20) |
| V7 | The Windows binary from the installed plugin on Ryan's machine. **Verified 2026-10-03** ([#13](https://github.com/Wet-Ink-Corporation/weigh-in/issues/13)): installed from the `marketplace` branch, hooks fired under Git Bash, a sitting decided an item. Hooks without Git Bash: exec-form hooks from 0.5.0 ([B-16](decisions/build-decisions.md#b-16)), spiked on Linux, macOS and Windows; checked from the 0.5.0 release on 2026-10-03, including Cowork keeping the launcher's executable bit; still to confirm a marker captured on a Windows session without Git Bash. The "no binary" failure of [#76](https://github.com/Wet-Ink-Corporation/weigh-in/issues/76) came from a claude.ai-synced copy of `main`, which has no binaries; only `weigh-in-plugin` is synced, and the launcher now names that case ([B-18](decisions/build-decisions.md#b-18)) | [#21](https://github.com/Wet-Ink-Corporation/weigh-in/issues/21) |

## Known gaps

- **The board URL lands in the tracked config:** publishing the board writes its URL into
  `.kb/_ledger/config.json`. Keep it local (`git update-index --skip-worktree`), because a
  committed URL would point every clone's ledger, cloud containers included, at one board
  ([B-20](decisions/build-decisions.md#b-20)).
- **No binaries on `main`:** install from the `marketplace` branch instead ([B-14](decisions/build-decisions.md#b-14));
  verified on Windows and in Cowork ([#13](https://github.com/Wet-Ink-Corporation/weigh-in/issues/13)); macOS and Linux installs are unverified.
  A plugin without a binary says capture is off ([B-11](decisions/build-decisions.md#b-11)).
- **Cowork download is dormant** while releases are internal ([D-0012](decisions/build-decisions.md#d-0012));
  Cowork uses a connected checkout of the `marketplace` branch ([#13](https://github.com/Wet-Ink-Corporation/weigh-in/issues/13)).
- **This repository's ledger does not outlive a cloud session** ([D-0014](decisions/build-decisions.md#d-0014)):
  cloud sessions settle or defer everything before they end. Ryan's own clones, now installed from
  the `marketplace` branch, keep theirs. Since [B-21](decisions/build-decisions.md#b-21) the
  event history survives as a committed journal per cloud session, which `weighin stats` folds
  in, so cloud sittings count toward [#23](https://github.com/Wet-Ink-Corporation/weigh-in/issues/23).
  Cloud sittings before the change are lost, apart from their decisions.
- **No board from Cowork:** slice 2 ([#27](https://github.com/Wet-Ink-Corporation/weigh-in/issues/27)).
- **happenstance is held at 0.3.2**, now with a way past it: a schema change is one more ledger
  migration step ([B-10](decisions/build-decisions.md#b-10)).
- **The default owner is `ryan`** unless `init --owner` sets it; after v1 ([#27](https://github.com/Wet-Ink-Corporation/weigh-in/issues/27)).
- **Sitting cost follows session length:** about 3 × the context at the cache-read price plus
  about 5,600 output tokens, recorded as the v1 figure ([D-0016](decisions/build-decisions.md#d-0016), [#24](https://github.com/Wet-Ink-Corporation/weigh-in/issues/24)).
- **Agents marked few of their own calls before 0.5.2.** On the eval seed (70 items, 19 labelled
  `capture`), recall is 1 of 7 before 0.5.2's one-block-per-call line and 11 of 12 after it;
  precision is 12 of 12 ([#22](https://github.com/Wet-Ink-Corporation/weigh-in/issues/22)). The
  sessions after 0.5.2 are few and all on this repository, so the sitting's sweep still catches
  what markers miss.
- **Open from wi-4d8b6a:** which provider the separately held model key is for; slice 2 ([#27](https://github.com/Wet-Ink-Corporation/weigh-in/issues/27)).

## Slice 2

The next milestone after v1. Not in v1 ([D-0013](decisions/build-decisions.md#d-0013)), except per-category suppression if
the not-useful rate exceeds 10% ([#25](https://github.com/Wet-Ink-Corporation/weigh-in/issues/25)).
The items, with the design's estimates, are in the [v1 plan](v1-plan.md#decisions) and tracked in
[#27](https://github.com/Wet-Ink-Corporation/weigh-in/issues/27).

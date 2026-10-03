# Roadmap and open items

State at 0.4.1 (2026-10-03). Slice 1 is built: capture, the in-chat sitting, decision records,
the Cowork route through the computer's shell, and the local marketplace. Phase 1 of the v1 plan
is done: the ledger has a schema version, SessionStart says when capture is off, both launchers
pick the newer build, and this repository uses Weigh-In on itself. Phase 2 is built: each release
publishes a `marketplace` branch Claude Code installs from without Rust, and
[`install.md`](install.md) is the install guide; the installs on Ryan's machines (#13) follow the
`v0.4.0` tag. The path from here to v1 is in [`v1-plan.md`](v1-plan.md).

Open work is tracked as GitHub issues ([D-0011](decisions/build-decisions.md#d-0011)), in the order of
the [v1 plan](v1-plan.md). This page summarises them; the issue is where status is kept.

| Phase | Issues |
|---|---|
| 0: organise the repository | [#5](https://github.com/Wet-Ink-Corporation/weigh-in/issues/5) rulesets on `main` and `v*` tags · [#28](https://github.com/Wet-Ink-Corporation/weigh-in/issues/28) scaffolding, CI coverage, issue tracking · [#6](https://github.com/Wet-Ink-Corporation/weigh-in/issues/6) move the temper research |
| 1: use it here, upgrade safety (0.3.0) · **done** | [#7](https://github.com/Wet-Ink-Corporation/weigh-in/issues/7) use Weigh-In on this repository · [#8](https://github.com/Wet-Ink-Corporation/weigh-in/issues/8) ledger schema version · [#9](https://github.com/Wet-Ink-Corporation/weigh-in/issues/9) SessionStart says when capture is off · [#10](https://github.com/Wet-Ink-Corporation/weigh-in/issues/10) `weighin.cmd` picks the newer build · [#11](https://github.com/Wet-Ink-Corporation/weigh-in/issues/11) release |
| 2: distribution (0.4.0) | [#12](https://github.com/Wet-Ink-Corporation/weigh-in/issues/12) `marketplace` branch from `release.yml` · [#13](https://github.com/Wet-Ink-Corporation/weigh-in/issues/13) install from it; Cowork through a connected checkout · [#14](https://github.com/Wet-Ink-Corporation/weigh-in/issues/14) install guide · [#15](https://github.com/Wet-Ink-Corporation/weigh-in/issues/15) release |
| 3: verify and use (0.9.x) | [#16](https://github.com/Wet-Ink-Corporation/weigh-in/issues/16) V1 · [#17](https://github.com/Wet-Ink-Corporation/weigh-in/issues/17) V2 · [#18](https://github.com/Wet-Ink-Corporation/weigh-in/issues/18) V4 · [#19](https://github.com/Wet-Ink-Corporation/weigh-in/issues/19) V5 · [#20](https://github.com/Wet-Ink-Corporation/weigh-in/issues/20) V6 · [#21](https://github.com/Wet-Ink-Corporation/weigh-in/issues/21) V7 and hooks without Git Bash · [#22](https://github.com/Wet-Ink-Corporation/weigh-in/issues/22) eval seed · [#23](https://github.com/Wet-Ink-Corporation/weigh-in/issues/23) two weeks of daily use · [#24](https://github.com/Wet-Ink-Corporation/weigh-in/issues/24) sitting cost · [#25](https://github.com/Wet-Ink-Corporation/weigh-in/issues/25) suppression, only if not-useful exceeds 10% |
| 4: v1.0 | [#26](https://github.com/Wet-Ink-Corporation/weigh-in/issues/26) release 1.0.0 and record v1 live |
| After v1 | [#27](https://github.com/Wet-Ink-Corporation/weigh-in/issues/27) slice 2 and later |

## Not yet verified

| Id | What | Issue |
|---|---|---|
| V1 | Session identity in Cowork: whether `${CLAUDE_SESSION_ID}` is substituted in a synced skill | [#16](https://github.com/Wet-Ink-Corporation/weigh-in/issues/16) |
| V2 | ArtifactData reads and writes from Cowork | [#17](https://github.com/Wet-Ink-Corporation/weigh-in/issues/17) |
| V4 | Editor links (`vscode://file/…`) opened from inside an Artifact | [#18](https://github.com/Wet-Ink-Corporation/weigh-in/issues/18) |
| V5 | Quotas of the board's `db` store | [#19](https://github.com/Wet-Ink-Corporation/weigh-in/issues/19) |
| V6 | Previews in the desktop Code tab and the Cowork question card | [#20](https://github.com/Wet-Ink-Corporation/weigh-in/issues/20) |
| V7 | The Windows binary from the installed plugin on Ryan's machine | [#21](https://github.com/Wet-Ink-Corporation/weigh-in/issues/21) |

## Known gaps

- **No binaries on `main`:** install from the `marketplace` branch instead ([B-14](decisions/build-decisions.md#b-14));
  verified on Ryan's machines under [#13](https://github.com/Wet-Ink-Corporation/weigh-in/issues/13).
  A plugin without a binary says capture is off ([B-11](decisions/build-decisions.md#b-11)).
- **Cowork download is dormant** while releases are internal ([D-0012](decisions/build-decisions.md#d-0012));
  Cowork uses a connected checkout of the `marketplace` branch ([#13](https://github.com/Wet-Ink-Corporation/weigh-in/issues/13)).
- **This repository's ledger does not outlive a cloud session** ([D-0014](decisions/build-decisions.md#d-0014)):
  cloud sessions settle or defer everything before they end; accepted until Phase 2's installs.
- **No board from Cowork:** slice 2 ([#27](https://github.com/Wet-Ink-Corporation/weigh-in/issues/27)).
- **happenstance is held at 0.3.2**, now with a way past it: a schema change is one more ledger
  migration step ([B-10](decisions/build-decisions.md#b-10)).
- **The default owner is `ryan`** unless `init --owner` sets it; after v1 ([#27](https://github.com/Wet-Ink-Corporation/weigh-in/issues/27)).
- **Sitting cost is above the estimate:** $0.42–0.57 against $0.28 ([#24](https://github.com/Wet-Ink-Corporation/weigh-in/issues/24)).
- **Open from wi-4d8b6a:** which provider the separately held model key is for; slice 2 ([#27](https://github.com/Wet-Ink-Corporation/weigh-in/issues/27)).

## Slice 2

Not in v1 ([D-0013](decisions/build-decisions.md#d-0013)), except per-category suppression if
the not-useful rate exceeds 10% ([#25](https://github.com/Wet-Ink-Corporation/weigh-in/issues/25)).
The items, with the design's estimates, are in the [v1 plan](v1-plan.md#decisions) and tracked in
[#27](https://github.com/Wet-Ink-Corporation/weigh-in/issues/27).

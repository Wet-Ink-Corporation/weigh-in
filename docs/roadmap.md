# Roadmap and open items

State at 0.2.1 (2026-10-01). Slice 1 is built: capture, the in-chat sitting, decision records,
the Cowork route through the computer's shell, and the local marketplace.

## Not yet verified

| Id | What | How to close it |
|---|---|---|
| V1 | Session identity in Cowork: whether `${CLAUDE_SESSION_ID}` is substituted in a synced skill | Run `/weigh-in` in Cowork and check the session id the skill passes |
| V2 | ArtifactData reads and writes from Cowork | One board round trip from a Cowork chat |
| V4 | Editor links (`vscode://file/…`) opened from inside an Artifact | Click one on the board |
| V5 | Quotas of the board's `db` store | Push a large ledger; record where it fails |
| V6 | Previews in the desktop Code tab and the Cowork question card | One sitting on each surface |
| V7 | The Windows binary from the installed plugin on Ryan's machine | Any `/weigh-in` sitting in Claude Code on Windows |

## Known gaps

- **Cowork download is dormant.** The skill downloads its binary from a public GitHub release
  ([D-0003](decisions/build-decisions.md#d-0003)); the repository is internal, so zips are packaged
  with `--no-download` and Cowork needs a connected folder holding this repository. Closing it:
  public releases (or a public release-only repository), then package without the flag.
- **No board from Cowork.** The skill uses in-chat cards there; the board route would need the
  state file staged through the bridge.
- **happenstance is held at 0.3.2.** The ledger copies happenstance's schema into a TRUNCATE
  database ([B-01](decisions/build-decisions.md#b-01)); a happenstance release with a schema
  change needs a migration step in `Store::open`.
- **`weighin.cmd` prefers the msvc build** whatever its age; the `sh` launcher prefers the newer
  build. Rebuild msvc with `scripts/build-plugin-bins.ps1` whenever the Rust code changes.
- **Paths are matched to the repository by resolving them**, so an existing file under an 8.3
  short name or a symlinked temp dir counts as in the repo. A file that does not exist yet, and
  an absolute `--at path:line` anchor (`ops.rs` passes the whole anchor), still compare as
  spelled. Closing it: resolve the nearest existing ancestor, and split the anchor first.
- **The default owner is `ryan`** (`human:ryan` in records) unless `init --owner` sets it.
- **Sitting cost is above the estimate:** $0.42–0.57 measured on Sonnet against $0.28, mostly the
  batch JSON and card output.
- **Open from wi-4d8b6a:** which provider the separately held model key is for. Any key is read
  from an environment variable at run time and never written to the ledger; judges stay
  Anthropic-only (Q3).

## Slice 2

From the [design](research/report.md), lower-bound estimates there:

- Sonnet grounder and the three-sample permuted stability check, in the CLI.
- Local review page (`weighin serve --wait`) for client repos, whose board is local
  ([wi-7c21e4](decisions/wi-7c21e4.md)).
- Per-category suppression of noisy item kinds.
- Just-in-time surfacing: show the relevant decision when an edit touches its anchor.
- Reliance drills ([wi-2b90f3](decisions/wi-2b90f3.md)).
- An independent door classifier for unattended runs ([wi-4d8b6a](decisions/wi-4d8b6a.md)).
- The Redkiln-native decision item type and kb-ingest of the intake keys
  ([wi-e15c77](decisions/wi-e15c77.md)), when redkiln-rs is ready to receive Weigh-In.

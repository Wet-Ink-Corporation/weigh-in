# Weigh-In

A decision ledger for agent sessions: the `weighin` CLI over happenstance's SQLite event store, shipped as a Claude Code and Cowork plugin whose skill is `/weigh-in`. Agents raise questions, assumptions and deferrals; the owner decides them on a board; decisions land in `.kb` as atoms. `CLAUDE.md` holds the repository's working rules; this file is the workspace charter temper reads.

## Crate map

| Crate | Layer | Purpose | Depends on |
|---|---|---|---|
| `crates/weighin-core` | domain | The ledger vocabulary with no I/O: events, `wi-` ids, `weigh-in` markers, scanning, cards, the projection fold, `.kb` atoms and the board exchange format. Linkable into redkiln-rs unchanged. | serde, serde_json, sha2, thiserror |
| `crates/weighin` | binary | The CLI and single writer of the ledger: the store (happenstance SQLite at `<git-common-dir>/weighin/`), git through the command line, context packs, the hook entry point, and the batch write path (`ops::Session`). | weighin-core, happenstance-core, happenstance-sqlite, tokio (rt), clap, anyhow, time, tempfile, serde, serde_json, sha2 |

Dependencies point inward: the binary depends on the domain, never the reverse (ARC-02). Each crate's `AGENTS.md` is its charter (ARC-08). The `plugin/` directory (skill, hooks, launcher) and `scripts/` are not crates.

## Commands

| Purpose | Command |
|---|---|
| Format | `cargo fmt --all` |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` (with the baseline: `node .claude/temper/temper.mjs baseline --check`) |
| Test | `cargo test --workspace` |
| Docs | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` |
| Supply chain | `cargo deny check` (`deny.toml`) |
| Plugin | `./scripts/build-plugin-bins.sh`, `./scripts/package-plugin.sh --no-download`, `claude plugin validate .` |
| Full gate (Claude Code) | `/temper:gate`, or `node "$TEMPER_CLI" gate` |

Toolchain 1.98.1 (`rust-toolchain.toml`), pinned to redkiln-rs's channel so `weighin-core` stays adoptable there (wi-b4c1d8). `rust-version` is 1.98.1, equal to the pin. Profiles are the repository's own (`[profile.release]`: thin LTO, one codegen unit, stripped); the template's profiles were not adopted (PERF-09).

## Engineering standard (rust-craft)

All Rust here follows rust-craft. The full text, with rule IDs, examples, the review rubric and the process, is the `rust-craft` skill shipped in the temper plugin. The non-negotiables for production code (tests may use `unwrap`/`expect`/`assert!`):

- H-01 no `unsafe`; H-02 no `unwrap`/`expect`; H-03 no `panic!`/`todo!`/`unimplemented!`/`unreachable!`, and no std API that panics on input the code does not control; H-04 no panicking indexing or slicing, including `map[&k]` and `deque[i]`, which no lint catches.
- H-05 no `#[allow]`; `#[expect(lint, reason)]` needs human approval; never weaken lint, CI or hook config.
- H-06 no `dbg!`, no print macros (use `tracing` or `writeln!` on a locked handle), no commented-out code, no untracked TODOs.
- H-07 no `Box::leak`/`mem::forget`/`transmute`/`static mut`/`'static` to silence lifetimes; H-08 no clones to appease the borrow checker.
- H-09 typed errors in library APIs (`thiserror`), with `anyhow` only in binaries; H-10 never discard failures; H-11 no lossy `as` casts.
- H-12 never weaken tests or checks, including manifest test switches (`test`, `doctest`, `harness`, `autotests`) and `#[ignore]` or `cfg_attr(.., ignore)` without a reason; H-13 bounded and owned resources.
- H-14 no blocking in async code, and no lock guard, `RefCell` borrow, entered tracing span or `watch::Ref` held across `.await`; H-15 zero warnings, documented public API.
- H-16 new dependencies and major-version bumps, `[patch]` overrides, `rust-version` raises, API breaks, lint exceptions, crate-boundary changes, migrations and test rewrites are human decisions.

Process: explore, frame, plan (`.temper/plans/`), red, green, refactor, self-review, gate, independent review, then report with a Status block (`DONE`, `WIP` or `BLOCKED`, plus the gate result, the review verdict, what changed, decisions needed and what was not verified).

## Decisions log

| Date | Decision | Rule | Decided by |
|---|---|---|---|
| 2026-10-02 | Adopt temper (pilot, temper plan 2.1): the template's `[workspace.lints]` tables replace the repository's lighter ones, and the existing findings are recorded in `.claude/temper-baseline.json`. `[workspace.package]` and the profiles are unchanged. | H-05, H-16 | Ryan Britton |

Settled product decisions live in `docs/decisions/` (see `CLAUDE.md`).

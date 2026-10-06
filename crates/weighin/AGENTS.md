# Crate charter: `weighin`

## Purpose
The `weighin` CLI, and the single writer of the Weigh-In decision ledger. Every write, from a command, a hook or the `batch` entry point, goes through `ops::Session`; read-only commands use `ops::read_ledger`, which never creates the store. That write path reads the ledger, validates, appends under a guard, and then performs the side effects of a decision, such as writing atoms and annotating sources.

## Layer
binary. The composition root holds the store, git, context packs, environment discovery and the Claude Code hook entry point. Output is written for a model to read at the lowest token cost: short lines, and `--json` where a structure is needed.

## Allowed dependencies
- Workspace crates: `weighin-core`.
- Third-party:
  - `happenstance-core` and `happenstance-sqlite`, the event store;
  - `tokio` with `rt` only, which happenstance's API needs;
  - `clap` (CLI), `anyhow` (binary errors), `time` (timestamps), `tempfile` (temporary indexes and files on the temp disk);
  - `serde` and `serde_json`.
- Never: a second writer of the ledger, or a git library. Git goes through the command line (`git.rs`).

## Public surface
None. This is a binary crate; its modules are private (`context`, `env`, `git`, `hooks`, `ops`, `store`).

## Invariants
- **One write path.** Every change to the ledger goes through `ops::Session`.
- **Conditioned writes (DCB).** Raising an item fails if that item already exists. Every later event for an item fails if the item changed since the writer read it, so two sessions cannot both decide the same item (`store.rs`).
- **No deletes beside the ledger.** The store runs under a TRUNCATE journal and never through happenstance's WAL open, and temporary files stay on the temp disk (B-01).
- **Hooks never fail a session.** Errors are appended to `<state dir>/hook-errors.log`, and the hook exits 0 with no output (`hooks.rs`).
- **Git is read-only, apart from a tree hash on a temporary index.** Nothing here commits, moves refs or runs hooks (`git.rs`).
- **Context packs stay bounded** by `caps.context_chars`, about 600 tokens (`context.rs`).

## Errors
`anyhow` throughout, with `Context` on I/O, store and git failures. Domain rejections from `weighin-core` (`CardError`, `AnswerError`) are reported to the caller as they are. Exit codes: 0 on success; any error prints `weighin: <error chain>` to stderr and exits 1, except a closed stdout (`weighin open | head`): a broken pipe on stdout, anywhere in the chain, exits 0 with nothing on stderr (a command that changes the ledger still applies every operation first, and only its receipts stop), while a broken pipe on stderr or elsewhere is an error like any other; `weighin hook` always exits 0, even on a panic, and logs its own output failures to `hook-errors.log`, and its input and parse failures there only inside a git repository; outside git those leave no trace, and in Cowork hooks write nothing (`main.rs`, `hooks.rs`).

## Test strategy
Unit tests sit beside the code in `env`, `store`, `context`, `git`, `hooks`, `ops`, `text` and `main`; the ones that spawn processes or open SQLite carry `#[cfg_attr(miri, ignore = "...")]`. End-to-end tests in `tests/it/cli.rs` cover:
- a scratch repository;
- hooks fed the JSON Claude Code sends;
- a linked worktree;
- a sitting done through `batch`;
- the board round trip.

Run with `cargo test -p weighin`.

## Do not
- Write to the ledger outside `ops::Session`.
- Create, rename or delete files beside the ledger (B-01).
- Let a hook return a non-zero exit code or print an error to the session.
- Ship binaries in a top-level `plugin/bin/` (B-03), or keep an old binary next to a rebuilt one (B-05).

## Decisions
| Date | Decision | Rule | Decided by |
|---|---|---|---|

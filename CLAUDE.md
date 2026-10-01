# Weigh-In

A decision ledger for agent sessions: a Rust CLI (`weighin`) over happenstance's SQLite event
store, shipped as a Claude Code / Cowork plugin whose skill is `/weigh-in`. Read `README.md` for
what it does, `docs/decisions/` before changing behaviour, and `docs/roadmap.md` for open items.

## Layout

- `crates/weighin-core` — no I/O: events, ids, markers, scanning, cards, projection, atoms, board format.
- `crates/weighin` — the CLI: store, git, context packs, hooks, the batch write path.
- `plugin/` — the plugin: `skills/weigh-in/SKILL.md`, `hooks/hooks.json`, `libexec/` launcher and built binaries.
- `scripts/` — `build-plugin-bins.sh` (Linux musl and Windows gnu), `build-plugin-bins.ps1` (Windows msvc), `package-plugin.sh` (zip for upload).
- `docs/` — research report and notes, decision records, roadmap.

## Commands

```sh
cargo test --workspace
cargo clippy --all-targets -- -D warnings
cargo fmt --check
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
./scripts/build-plugin-bins.sh               # binaries into plugin/libexec/<target>/
./scripts/package-plugin.sh --no-download    # dist/weigh-in-plugin-<version>.zip
claude plugin validate .                     # marketplace and plugin manifests
```

These four are what `.github/workflows/ci.yml` runs on every push to `main` and every pull
request, on Linux and Windows; the release workflow runs the same checks before it builds.
Dependabot (`.github/dependabot.yml`) opens weekly update PRs, holding back Cargo majors and
happenstance (B-01).

Toolchain is pinned to redkiln-rs's (Rust 1.98.1, edition 2024), because redkiln-rs is where
Weigh-In ends up (wi-b4c1d8). Line endings are LF everywhere (`.gitattributes`).

## Rules learned the hard way

- **No top-level `plugin/bin/`.** claude.ai rejects hosted plugins that ship one; binaries live in `plugin/libexec/` and are called by path (B-03).
- **No deletes beside the ledger.** Cowork's connected folders refuse deletes; the store runs under a TRUNCATE journal and never goes through happenstance's WAL open (B-01). Keep temporary files on the temp disk, not beside the ledger or in the repo.
- **Cowork is not Claude Code.** Skills and hooks run in a scratch cloud workspace (`CLAUDE_CODE_ENTRYPOINT=remote_cowork`); `!` injection does not run; edits to the user's folder go through the computer's shell, so `PostToolUse` never sees them. The skill's Cowork route runs `weighin` through that shell (D-0002).
- **Skill names must stand alone.** Cowork lists plugin skills by the skill's own name; Claude Code lists `/<plugin>:<skill>` and also resolves the bare name (D-0004).
- **Versions move together.** `Cargo.toml` and `plugin/.claude-plugin/plugin.json` carry the same version; bump both for any plugin change, or `/plugin marketplace update` will not pick it up.
- **One stamp in the skill.** `SKILL.md` holds exactly one `V=…; S=…;`; `package-plugin.sh` stamps it in the zip only. The repository copy stays `V=dev; S=none;`.
- **Rebuild every binary you keep** after a Rust change: an old build next to a new one reopens the ledger under its own journal mode (B-05).

## Decisions

Settled decisions are not reopened without new evidence. Record new ones in
`docs/decisions/` (board sittings as one file per `wi-` id, chat decisions in
`build-decisions.md`), saying who took them and what they supersede.

## Releasing

Bump both versions, commit to `main`, push the tag `v<version>`. The workflow tests, builds Linux,
Windows and macOS binaries, packages the plugin and publishes the release. While the repository
is internal it packages with `--no-download` (D-0007).

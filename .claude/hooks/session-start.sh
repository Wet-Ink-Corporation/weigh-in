#!/bin/bash
# Claude Code cloud sessions: install the pinned toolchain (rust-toolchain.toml) and build the
# workspace's tests and lints once, so the gate in CLAUDE.md runs straight away. Local sessions
# skip it. Idempotent; the container is cached after it completes.
set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

cd "$CLAUDE_PROJECT_DIR"
rustup toolchain install >&2
cargo fetch --locked >&2
cargo test --workspace --no-run --locked >&2
cargo clippy --workspace --all-targets --locked >&2

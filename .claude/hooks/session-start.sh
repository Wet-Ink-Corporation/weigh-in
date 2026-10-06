#!/bin/bash
# Claude Code cloud sessions: install the pinned toolchain (rust-toolchain.toml) and build the
# workspace's tests and lints once, so the gate in CLAUDE.md runs straight away. Then set up
# Weigh-In on this repository (D-0010, B-12): a cloud session does not install the plugins a
# repository enables, so build the binary into plugin/libexec/, install the plugin's skill as a
# project skill, and run Weigh-In's own SessionStart here, after the build, rather than as a
# separate hook that would run alongside this one. Local sessions skip it all. Idempotent; the
# container is cached after it completes.
set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

payload=$(cat)
cd "$CLAUDE_PROJECT_DIR"
rustup toolchain install >&2
# Warming the caches never stops the Weigh-In setup below: a branch whose tests do not build
# (an agent mid-fix) still gets /weigh-in and its hooks.
cargo fetch --locked >&2 || echo "session-start: cargo fetch failed" >&2
cargo test --workspace --no-run --locked >&2 \
  || echo "session-start: the tests do not build; the gate in CLAUDE.md will say why" >&2
# Lints warm clippy's cache only. temper's lint tables deny findings the baseline records, so
# plain clippy fails; the gate (temper.mjs baseline --check) judges them, not this hook (B-13).
cargo clippy --workspace --all-targets --locked --keep-going >&2 \
  || echo "session-start: clippy reports findings; the gate is node .claude/temper/temper.mjs baseline --check" >&2

# The binary, where the launcher looks for it (git ignores plugin/libexec/*/).
# When it does not build, the launcher's SessionStart below says capture is off.
target="$(uname -m)-unknown-linux-gnu"
if cargo build -p weighin --locked >&2; then
  mkdir -p "plugin/libexec/$target"
  cp target/debug/weighin "plugin/libexec/$target/weighin"
else
  # A build kept from an earlier session in this container would still capture, with old code.
  rm -f "plugin/libexec/$target/weighin"
  echo "session-start: weighin does not build, so capture is off this session" >&2
fi

# The skill, as a project skill (git ignores it). ${CLAUDE_PLUGIN_ROOT} is set only for plugins.
skill=.claude/skills/weigh-in
rm -rf "$skill"
mkdir -p .claude/skills
cp -R plugin/skills/weigh-in "$skill"
sed -i 's#${CLAUDE_PLUGIN_ROOT}#${CLAUDE_PROJECT_DIR}/plugin#g' "$skill/SKILL.md"

# Weigh-In's SessionStart (the marker convention, or a message that capture is off), with the
# skill directories re-scanned so /weigh-in is there from the first prompt.
out=$(printf '%s' "$payload" | sh plugin/libexec/weighin hook SessionStart 2>/dev/null || true)
OUT="$out" python3 - <<'PY'
import json, os

out = os.environ.get("OUT", "")
try:
    msg = json.loads(out)
    if not isinstance(msg, dict):
        raise ValueError
except ValueError:
    msg = {"hookSpecificOutput": {"additionalContext": out}} if out.strip() else {}
hso = msg.setdefault("hookSpecificOutput", {})
hso["hookEventName"] = "SessionStart"
hso["reloadSkills"] = True
print(json.dumps(msg))
PY

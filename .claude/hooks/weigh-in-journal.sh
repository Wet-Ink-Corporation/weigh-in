#!/bin/sh
# Weigh-In's pilot evidence from Claude Code cloud sessions on this repository (#23, B-21). The
# ledger goes with the container (D-0014), so after every Bash call (a sitting's `batch`) and at
# every Stop this writes the ledger's events to .kb/_ledger/journal/<session>.jsonl, and any
# hook-errors.log lines beside it. The session commits that directory with its work, and
# `weighin stats` folds every journal with the ledger. Local sessions keep their ledger, so this
# does nothing there. It never fails a session: every failure exits 0.
[ "${CLAUDE_CODE_REMOTE:-}" = "true" ] || exit 0
cat >/dev/null
cd "$CLAUDE_PROJECT_DIR" 2>/dev/null || exit 0
session=${CLAUDE_CODE_SESSION_ID:-}
case "$session" in
  "" | */* | .*) exit 0 ;;
esac
common=$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null) || exit 0
state="$common/weighin"
# Nothing to keep until the first item is raised or a hook logs an error.
[ -f "$state/ledger.sqlite3" ] || [ -s "$state/hook-errors.log" ] || exit 0

dir=.kb/_ledger/journal
mkdir -p "$dir" || exit 0
# Append-only: lines already in the journal stay, and only events not yet in it are added. A
# session resumed in a new container keeps its id but starts an empty ledger, and must not wipe
# the events its journal already holds. `weighin journal` writes each event the same way every
# time, so an event already kept matches its line exactly.
journal="$dir/$session.jsonl"
tmp=$(mktemp) || exit 0
if sh plugin/libexec/weighin journal >"$tmp" 2>/dev/null && [ -s "$tmp" ]; then
  if [ -s "$journal" ]; then
    grep -Fxv -f "$journal" "$tmp" >>"$journal"
  else
    cp "$tmp" "$journal"
  fi
fi
rm -f "$tmp"
errors="$dir/$session.hook-errors.log"
if [ -s "$state/hook-errors.log" ]; then
  if [ -s "$errors" ]; then
    grep -Fxv -f "$errors" "$state/hook-errors.log" >>"$errors"
  else
    cp "$state/hook-errors.log" "$errors"
  fi
fi
exit 0

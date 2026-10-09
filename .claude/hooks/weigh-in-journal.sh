#!/bin/sh
# Weigh-In's pilot evidence from Claude Code cloud sessions on this repository (#23, B-21). The
# ledger goes with the container (D-0014), so after every Bash call (a sitting's `batch`) and
# after every capture hook this writes the ledger's events to .kb/_ledger/journal/<session>.jsonl,
# and any hook-errors.log lines beside it. The session commits that directory with its work, and
# `weighin stats` folds every journal with the ledger. Local sessions keep their ledger, so this
# does nothing there. It never fails a session: every failure exits 0.
[ "${CLAUDE_CODE_REMOTE:-}" = "true" ] || exit 0
payload=$(cat)
cd "$CLAUDE_PROJECT_DIR" 2>/dev/null || exit 0
session=${CLAUDE_CODE_SESSION_ID:-$(printf '%s' "$payload" \
  | sed -n 's/.*"session_id" *: *"\([^"]*\)".*/\1/p' | head -n 1)}
case "$session" in
  "" | */* | .*) exit 0 ;;
esac
common=$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null) || exit 0
state="$common/weighin"
# Nothing to keep until the first item is raised or a hook logs an error.
[ -f "$state/ledger.sqlite3" ] || [ -s "$state/hook-errors.log" ] || exit 0

dir=.kb/_ledger/journal
mkdir -p "$dir" || exit 0
work=$(mktemp -d) || exit 0
trap 'rm -rf "$work"' EXIT

# Hooks run in parallel (capture and the Bash hook, parallel tool calls), so one update at a time.
# The wait is at most 3s, so capture plus the wait stays inside the 10s hook timeout; a skipped
# run is caught up by the next. The lock lives on the temp disk, not beside the ledger (B-01).
exec 9>"${TMPDIR:-/tmp}/weigh-in-journal-$session.lock" || exit 0
flock -w 3 9 || exit 0

# Append-only: lines already kept stay, and only lines not yet kept are added, in one write. A
# session resumed in a new container keeps its id but starts an empty ledger, and must not wipe
# what its journal already holds. `weighin journal` writes each event the same way every time, so
# a kept event matches its line exactly. Every write ends in a newline, so a last line without
# one was cut short by a run killed mid-write: it is dropped, and the event it held is appended
# again from the ledger, rather than kept to fail `stats` until someone repairs it.
keep() { # <new lines> <kept file>
  [ -s "$1" ] || return 0
  if [ -s "$2" ]; then
    [ -z "$(tail -c 1 "$2")" ] || sed -i '$d' "$2"
    grep -Fxv -f "$2" "$1" >"$work/add"
    [ -s "$work/add" ] && cat "$work/add" >>"$2"
  else
    cp "$1" "$2"
  fi
}
if [ -f "$state/ledger.sqlite3" ] && sh plugin/libexec/weighin journal >"$work/events" 2>/dev/null; then
  keep "$work/events" "$dir/$session.jsonl"
fi
[ -f "$state/hook-errors.log" ] && keep "$state/hook-errors.log" "$dir/$session.hook-errors.log"
exit 0

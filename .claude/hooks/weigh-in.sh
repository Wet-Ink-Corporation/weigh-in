#!/bin/sh
# Weigh-In on this repository in Claude Code cloud sessions (D-0010, B-12). A cloud session does not
# install the plugins a repository enables, so the plugin's capture hooks are wired here and run its
# launcher from the checkout; session-start.sh builds the binary and installs the skill. Local
# sessions use the installed plugin, so this does nothing there and nothing is captured twice.
# After capture, the session's journal is brought up to date (weigh-in-journal.sh, B-21), so it
# holds what this hook just raised or defaulted.
[ "${CLAUDE_CODE_REMOTE:-}" = "true" ] || exit 0
payload=$(cat)
printf '%s' "$payload" | sh "$CLAUDE_PROJECT_DIR/plugin/libexec/weighin" hook "$@"
status=$?
printf '%s' "$payload" | sh "$CLAUDE_PROJECT_DIR/.claude/hooks/weigh-in-journal.sh"
exit "$status"

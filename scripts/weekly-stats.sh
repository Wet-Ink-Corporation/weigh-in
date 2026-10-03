#!/bin/sh
# Weekly Weigh-In numbers for #23, as markdown to paste into the issue: `weighin stats` for each
# folder given (a git repository or a Cowork folder), a pooled row, and the hook-error count of
# each ledger. Stats are cumulative over a ledger's life, so each week's comment carries totals
# to date; the change since last week is the difference between two comments.
#
#   scripts/weekly-stats.sh ~/src/weigh-in ~/src/other-repo ~/Cowork/folder
#
# The binary is $WEIGHIN, else the newest installed plugin's launcher, else `weighin` on PATH.
# Folders whose ledger was moved with `init --ledger` are read through that setting by weighin,
# but their hook-error count is looked for at the default location only.
set -eu

if [ $# -eq 0 ]; then
  echo "usage: $0 <folder>..." >&2
  exit 2
fi

launcher=""
if [ -z "${WEIGHIN:-}" ]; then
  # Version directories sort by name; take the last, which is the newest for x.y.z with one-digit
  # parts and is otherwise only a guess: set WEIGHIN to be sure.
  for l in "$HOME"/.claude/plugins/cache/weigh-in/weigh/*/libexec/weighin; do
    [ -f "$l" ] && launcher=$l
  done
fi
w() {
  if [ -n "${WEIGHIN:-}" ]; then "$WEIGHIN" "$@"
  elif [ -n "$launcher" ]; then sh "$launcher" "$@"
  else weighin "$@"
  fi
}

# One integer field from `stats --json`, which is a flat object of integers (`null` when unset).
field() {
  printf '%s' "$1" | sed -n "s/.*\"$2\":\([0-9]*\).*/\1/p"
}
pct() {
  if [ "$2" -eq 0 ]; then printf '—'; else awk -v n="$1" -v d="$2" 'BEGIN { printf "%.0f%%", 100 * n / d }'; fi
}

version=$(w --version 2>/dev/null || echo "weighin: not found")
echo "**Week of $(date -u +%Y-%m-%d)** · $version"
echo
echo "| Ledger | Presented | Action rate | Not useful | Overrides | Median to decide | Unqualified deferrals | Unreviewed defaults | Hook errors |"
echo "|---|---|---|---|---|---|---|---|---|"

tp=0 ta=0 tn=0 to=0 tk=0 td=0 tu=0 te=0
for dir in "$@"; do
  name=$(basename "$dir")
  if common=$(git -C "$dir" rev-parse --path-format=absolute --git-common-dir 2>/dev/null); then
    state="$common/weighin"
  else
    state="$dir/.weighin"
  fi
  # weighin opens (and so creates) a ledger to read it; skip a folder that has none rather than
  # leave an empty one behind, unless its config names a ledger elsewhere.
  if [ ! -f "$state/ledger.sqlite3" ] && ! grep -qs '"store"' "$dir/.kb/_ledger/config.json"; then
    echo "| $name | no ledger | | | | | | | |"
    continue
  fi
  if ! j=$(w --dir "$dir" stats --json 2>/dev/null); then
    echo "| $name | stats failed | | | | | | | |"
    continue
  fi
  p=$(field "$j" presented); a=$(field "$j" acted); n=$(field "$j" not_useful)
  o=$(field "$j" chosen_against_recommendation); k=$(field "$j" chosen_with_recommendation_known)
  m=$(field "$j" median_seconds_to_decide); d=$(field "$j" deferred_unqualified)
  u=$(field "$j" defaults_unacknowledged)
  log="$state/hook-errors.log"
  e=0
  [ -f "$log" ] && e=$(wc -l < "$log" | tr -d ' ')
  echo "| $name | $p | $(pct "$a" "$p") | $(pct "$n" "$p") | $o of $k | ${m:+${m}s} | $d | $u | $e |"
  tp=$((tp + p)) ta=$((ta + a)) tn=$((tn + n)) to=$((to + o)) tk=$((tk + k))
  td=$((td + d)) tu=$((tu + u)) te=$((te + e))
done

echo "| **All** | **$tp** | **$(pct "$ta" "$tp")** | **$(pct "$tn" "$tp")** | $to of $tk | | $td | $tu | $te |"
echo
echo "Targets: action rate ≥60%, not useful ≤10% (above it, #25 comes into v1), at least 30 presented and two weeks."
echo "Sitting cost this week: _fill in_ (\`/cost\` at the end of a sitting, or the usage in its transcript)."
echo "Hook errors: _explain any non-zero count, or truncate the log once read._"

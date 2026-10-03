#!/bin/sh
# Build the `marketplace` branch's single commit from a packaged plugin zip (D-0009).
#
#   scripts/marketplace-branch.sh dist/weigh-in-plugin-<version>.zip <dir>
#
# <dir> must not exist. It becomes a fresh git repository holding one commit:
#   .claude-plugin/marketplace.json   this checkout's, whose plugin source is ./plugin
#   plugin/                           the zip unpacked: stamped skill, hooks, launchers and the
#                                     prebuilt binaries in libexec/<target>/, committed executable
# Claude Code installs from it with `/plugin marketplace add Wet-Ink-Corporation/weigh-in#marketplace`,
# and a connected checkout of it gives Cowork the Linux binary. The release workflow force-pushes
# the commit, so the branch never accumulates old binaries (B-14). Prints the commit id; never
# pushes, so it can be run and inspected locally.
set -eu
[ $# -eq 2 ] || { echo "usage: $0 <plugin zip> <new dir>" >&2; exit 2; }
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
zip=$1
out=$2
[ -f "$zip" ] || { echo "no such zip: $zip" >&2; exit 1; }
[ ! -e "$out" ] || { echo "$out already exists" >&2; exit 1; }

mkdir -p "$out/.claude-plugin" "$out/plugin"
cp "$root/.claude-plugin/marketplace.json" "$out/.claude-plugin/"
unzip -q "$zip" -d "$out/plugin"

cd "$out"
grep -q '"source": *"\./plugin"' .claude-plugin/marketplace.json \
  || { echo "marketplace.json must point its plugin at ./plugin" >&2; exit 1; }
# claude.ai rejects hosted plugins with a top-level bin/ (B-03).
[ ! -e plugin/bin ] || { echo "plugin/bin/ must not exist; binaries live in plugin/libexec/" >&2; exit 1; }
[ -f plugin/libexec/weighin ] || { echo "the zip has no libexec/weighin launcher" >&2; exit 1; }
bins=$(ls plugin/libexec/*/weighin plugin/libexec/*/weighin.exe 2>/dev/null || true)
[ -n "$bins" ] || { echo "the zip has no binaries under libexec/<target>/" >&2; exit 1; }
# Windows hooks start libexec/weighin.exe directly (B-16).
if ls plugin/libexec/*-windows-*/weighin.exe >/dev/null 2>&1; then
  [ -f plugin/libexec/weighin.exe ] || { echo "the zip has a Windows build but no libexec/weighin.exe" >&2; exit 1; }
  bins="$bins plugin/libexec/weighin.exe"
fi
version=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' plugin/.claude-plugin/plugin.json)
[ -n "$version" ] || { echo "plugin.json has no version" >&2; exit 1; }

git init -q
git checkout -q -b marketplace
# -f: a global or system gitignore must not drop the binaries.
git add -f .
# Executable whatever the zip or the filesystem kept: hooks start the launcher with no shell
# (exec form, B-16), and the launcher execs the binary.
for f in plugin/libexec/weighin $bins; do
  git update-index --chmod=+x "$f"
done
GIT_AUTHOR_NAME=github-actions GIT_AUTHOR_EMAIL=github-actions@users.noreply.github.com \
GIT_COMMITTER_NAME=github-actions GIT_COMMITTER_EMAIL=github-actions@users.noreply.github.com \
  git -c commit.gpgsign=false commit -q -m "Weigh-In $version marketplace"
git rev-parse HEAD

#!/bin/sh
# Zip plugin/ with its prebuilt binaries into dist/, the package Customize > Plugins > upload
# takes (Cowork and claude.ai sync plugins from the account, not from a local marketplace).
#
# The zipped skill is stamped with the plugin version and the SHA-256 of the Linux (musl)
# binary: in Cowork the skill downloads that exact binary from the GitHub release v<version>.
# So the zip and the release's `weighin-x86_64-unknown-linux-musl` must come from the same
# build. The release workflow runs this script for that reason; a zip packaged from a local
# build only matches a release made from that same build.
#
# `--no-download` stamps no checksum, so the skill never tries GitHub and uses a copy of the
# binary in a connected folder instead: for a zip that has no matching release (a local
# marketplace, a test build).
#
# Run scripts/build-plugin-bins.sh (and build-plugin-bins.ps1 on Windows) first.
set -eu
no_download=false
[ "${1:-}" = "--no-download" ] && no_download=true
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"
version=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' plugin/.claude-plugin/plugin.json)
# claude.ai rejects hosted plugins with a top-level bin/ (Claude Code puts it on PATH).
[ ! -e plugin/bin ] || { echo "plugin/bin/ must not exist: claude.ai rejects it; binaries live in plugin/libexec/" >&2; exit 1; }
ls plugin/libexec/*/weighin* >/dev/null 2>&1 || { echo "no binaries under plugin/libexec/<target>/; build them first" >&2; exit 1; }

musl=plugin/libexec/x86_64-unknown-linux-musl/weighin
if $no_download; then
  sha=none
elif [ -f "$musl" ]; then
  if command -v sha256sum >/dev/null 2>&1; then sha=$(sha256sum "$musl" | cut -d' ' -f1)
  else sha=$(shasum -a 256 "$musl" | cut -d' ' -f1); fi
else
  sha=none
fi

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
cp -R plugin/. "$stage/"
skill="$stage/skills/weigh-in/SKILL.md"
[ "$(grep -c 'V=[^;]*; S=[^;]*;' "$skill")" = 1 ] || { echo "SKILL.md must hold exactly one 'V=…; S=…;' stamp" >&2; exit 1; }
sed "s/V=[^;]*; S=[^;]*;/V=$version; S=$sha;/" "$skill" > "$skill.new" && mv "$skill.new" "$skill"

mkdir -p dist
out="$root/dist/weigh-in-plugin-$version.zip"
rm -f "$out"
(cd "$stage" && zip -qr "$out" . -x '*.DS_Store')
echo "dist/weigh-in-plugin-$version.zip · stamped v$version · linux sha256 $sha"

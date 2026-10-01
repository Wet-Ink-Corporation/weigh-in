#!/bin/sh
# Build the release binaries the plugin launcher looks for, into plugin/libexec/<target>/.
# Linux (static, musl) and Windows (mingw) cross-build from Linux; on Windows run
#   cargo build --release -p weighin && copy target\release\weighin.exe plugin\libexec\x86_64-pc-windows-msvc\
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"
build() {
  target=$1; shift
  rustup target add "$target" >/dev/null
  env "$@" cargo build --release --target "$target" -p weighin
  mkdir -p "plugin/libexec/$target"
  for f in weighin weighin.exe; do
    [ -f "target/$target/release/$f" ] && cp "target/$target/release/$f" "plugin/libexec/$target/"
  done
  echo "built plugin/libexec/$target"
}
case "${1:-all}" in
  linux|all) build x86_64-unknown-linux-musl CC_x86_64_unknown_linux_musl=musl-gcc ;;
esac
case "${1:-all}" in
  windows|all) build x86_64-pc-windows-gnu CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc ;;
esac

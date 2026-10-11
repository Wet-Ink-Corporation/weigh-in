# Build the Windows binary the plugin launcher looks for (run from the repo root in PowerShell).
# "Stop" does not cover native commands: each one's exit code is checked, so a failed build never
# copies a stale exe.
$ErrorActionPreference = "Stop"
$target = "x86_64-pc-windows-msvc"
cargo build --locked --release -p weighin
if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
New-Item -ItemType Directory -Force -Path "plugin/libexec/$target" | Out-Null
Copy-Item "target/release/weighin.exe" "plugin/libexec/$target/weighin.exe" -Force
# Hooks start libexec/weighin with no shell, which resolves to libexec/weighin.exe (B-16); the
# build made last is the one there, as the launchers prefer the newer build (B-05).
Copy-Item "target/release/weighin.exe" "plugin/libexec/weighin.exe" -Force
Write-Host "built plugin/libexec/$target/weighin.exe and plugin/libexec/weighin.exe"

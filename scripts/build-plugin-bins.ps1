# Build the Windows binary the plugin launcher looks for (run from the repo root in PowerShell).
$ErrorActionPreference = "Stop"
$target = "x86_64-pc-windows-msvc"
cargo build --release -p weighin
New-Item -ItemType Directory -Force -Path "plugin/libexec/$target" | Out-Null
Copy-Item "target/release/weighin.exe" "plugin/libexec/$target/weighin.exe" -Force
Write-Host "built plugin/libexec/$target/weighin.exe"

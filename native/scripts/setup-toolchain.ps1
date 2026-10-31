# Prepares this machine to build Canopy's native apps with the GNU Rust toolchain, which has no usable dlltool.
# Recent Windows crates need one to create import libraries. LLVM's archiver acts as dlltool when its file is named
# dlltool.exe; this copies it (with the DLLs it loads) to %LOCALAPPDATA%\canopy-toolchain, which .cargo\config.toml
# points rustc at. rust-lld (also in .cargo\config.toml) links the result.
$ErrorActionPreference = "Stop"
rustup component add llvm-tools
$sysroot = (rustc --print sysroot).Trim()
$bin = Join-Path $sysroot "lib\rustlib\x86_64-pc-windows-gnu\bin"
$tools = Join-Path $env:LOCALAPPDATA "canopy-toolchain"
New-Item -ItemType Directory -Force $tools | Out-Null
Copy-Item (Join-Path $bin "llvm-ar.exe") (Join-Path $tools "dlltool.exe") -Force
Get-ChildItem $bin -Filter *.dll | Copy-Item -Destination $tools -Force
Write-Host "dlltool ready in $tools"

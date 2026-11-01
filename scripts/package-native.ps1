# Puts the native apps and what they need in one folder, ready for the owner to test before anything is installed:
#   package-native.ps1 [-Out <folder>] [-Build]
# -Build makes fresh release builds first (about 4 minutes each from scratch). The folder holds:
#   Canopy.exe, StreamCuts.exe, Forma.exe, Folio.exe   each app also runs alone
#   backend\long-to-short\python\transcribe.py          StreamCuts' captions and Boss AI (found above the exe)
#   assets\references\                                   Folio's Figma references (found above the exe)
#   README.txt                                           what the apps need on the computer
# It never writes to D:\Software and never replaces the installed Electron Canopy; the owner does that after testing.
param([string]$Out = "D:\projects\canopy\native\dist\Canopy", [switch]$Build)
$ErrorActionPreference = "Stop"
if ($Out -like "D:\Software*") { throw "Packaging into D:\Software is the owner's step, after testing." }
$canopy = "D:\projects\canopy\native"
$streamcuts = "D:\projects\streamcuts"
$forma = "D:\projects\forma-studio"
$folio = "D:\projects\folio-carousel-studio"
if ($Build) {
  Push-Location "$canopy"; cargo build --release; if ($LASTEXITCODE) { throw "Canopy build failed" }; Pop-Location
  Push-Location "$streamcuts\native"; cargo build --release; if ($LASTEXITCODE) { throw "StreamCuts build failed" }; Pop-Location
  # Forma and Folio alone; Forma keeps its own target folder so its checkout stays clean.
  Push-Location "$forma\native"; $env:CARGO_TARGET_DIR = "$canopy\target\forma-standalone"; cargo build --release; if ($LASTEXITCODE) { throw "Forma build failed" }; Pop-Location
  Push-Location "$folio\native"; $env:CARGO_TARGET_DIR = "$canopy\target"; cargo build --release; if ($LASTEXITCODE) { throw "Folio build failed" }; Pop-Location
  Remove-Item Env:CARGO_TARGET_DIR
}
$apps = [ordered]@{
  "Canopy.exe" = "$canopy\target\release\canopy.exe"
  "StreamCuts.exe" = "$streamcuts\native\target\release\streamcuts.exe"
  "Forma.exe" = "$canopy\target\forma-standalone\release\forma-native.exe"
  "Folio.exe" = "$canopy\target\release\folio-native.exe"
}
foreach ($exe in $apps.Values) { if (-not (Test-Path $exe)) { throw "Missing $exe; run with -Build" } }
New-Item -ItemType Directory -Force "$Out" | Out-Null
foreach ($name in $apps.Keys) { Copy-Item $apps[$name] (Join-Path $Out $name) -Force }
$python = Join-Path $Out "backend\long-to-short\python"
New-Item -ItemType Directory -Force $python | Out-Null
Copy-Item "$streamcuts\backend\long-to-short\python\transcribe.py", "$streamcuts\backend\long-to-short\python\requirements.txt" $python -Force
$references = Join-Path $Out "assets\references"
New-Item -ItemType Directory -Force $references | Out-Null
Copy-Item "$folio\assets\references\*" $references -Force
@"
Canopy native, packaged $(Get-Date -Format "yyyy-MM-dd HH:mm") from the Canopy branches (Canopy dev, the apps' canopy).

Canopy.exe runs Forma, Folio and StreamCuts together in shared spaces (the same spaces as the Electron Canopy).
Each app also runs alone from its own exe. Keep the folders next to the exes.

The computer also needs:
- FFmpeg and FFprobe on PATH, for StreamCuts and for Forma's video layers and video export.
  A shared FFmpeg build starts much faster than the static one.
- Python with faster-whisper (backend\long-to-short\python\requirements.txt), for StreamCuts' captions and Boss AI.
- GEMINI_API_KEY in the environment, for StreamCuts' Boss AI (finding shorts in a long video).
"@ | Set-Content -Path (Join-Path $Out "README.txt") -Encoding utf8
Get-ChildItem $Out | Select-Object Name, @{ n = "MB"; e = { if ($_.PSIsContainer) { [math]::Round((Get-ChildItem $_.FullName -Recurse -File | Measure-Object Length -Sum).Sum / 1MB, 1) } else { [math]::Round($_.Length / 1MB, 1) } } } | Format-Table -AutoSize
Write-Host "Packaged in $Out. Nothing was installed."

# Hands the Canopy native rewrite to Codex (full access) in this terminal.
# Codex reads HANDOFF.md, continues from the last commit, and keeps HANDOFF.md current, so Claude can take over again.
# The prompt is one line without quote marks: Windows PowerShell mangles both when it passes them to codex.
# codex exec runs unattended (no folder-trust prompt, no waiting for input) and exits when Codex stops,
# so the tab going idle means Codex is done. Its last message lands in %LOCALAPPDATA%\canopy-toolchain\codex-last.txt.
# Models, in order: GPT-6.1 Terra is the one the owner asked for, but on 2026-11-01 the Codex CLI on this ChatGPT
# account refused it (model_not_found: only the desktop app lists it), so a run that fails within a minute moves on
# to the CLI's own default, GPT-6 Sol. CANOPY_CODEX_MODEL picks one model and skips the list.
$ErrorActionPreference = "Stop"
$prompt = "You are taking over the Canopy native Rust rewrite from Claude, whose usage quota ran out; Claude takes over again when its quota resets, so leave everything in a state another agent can pick up. " +
  "Step 1: read D:\projects\canopy\HANDOFF.md first, then D:\projects\canopy\AGENTS.md. HANDOFF.md says what is done, the next steps in order, how to build and test, and the rules (branches, never merge into main, never edit the OneDrive StreamCuts worktree). " +
  "Step 2: set the Active agent line in HANDOFF.md to Codex and commit that. " +
  "Step 3: work through the next steps in order. Build and test after each one, and commit after each step that builds, on the branches HANDOFF.md names, with a clear message. " +
  "Step 4: before you stop for any reason, update HANDOFF.md (done, in progress, next steps, anything surprising), set the Active agent line to none, and commit."
Set-Location D:\projects\canopy
$last = Join-Path $env:LOCALAPPDATA "canopy-toolchain\codex-last.txt"
$models = if ($env:CANOPY_CODEX_MODEL) { @($env:CANOPY_CODEX_MODEL) } else { @("gpt-6.1-terra", "gpt-6-sol") }
foreach ($model in $models) {
  Write-Host "Starting Codex with $model"
  $started = Get-Date
  codex exec -m $model --dangerously-bypass-approvals-and-sandbox -C D:\projects\canopy --add-dir D:\projects\streamcuts -o $last $prompt
  if ($LASTEXITCODE -eq 0 -or ((Get-Date) - $started).TotalSeconds -gt 60) { break }
  Write-Host "Codex stopped at once with $model; trying the next model"
}

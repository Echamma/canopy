# Hands the Canopy native rewrite to Codex (full access).
# Codex reads HANDOFF.md, continues from the last commit, and keeps HANDOFF.md current, so Claude can take over again.
# The prompt is one line without quote marks: Windows PowerShell mangles both when it passes them to codex.
#
# First choice: the owner's Codex session in the ChatGPT desktop app, whose id is in
# %LOCALAPPDATA%\canopy-toolchain\codex-thread.txt (or CANOPY_CODEX_THREAD). `codex queue` hands the prompt to the
# app's local app-server daemon, which starts a turn in that session with the model chosen in the app (GPT-6.1 Terra
# runs there; the CLI can't use it on this ChatGPT account). The command returns at once and Codex works in the app,
# so this tab going idle says nothing: HANDOFF.md's Active agent line says whether Codex is still working.
#
# Fallback, when there's no session id or queueing fails: `codex exec` here, unattended, exiting when Codex stops
# (its last message lands in %LOCALAPPDATA%\canopy-toolchain\codex-last.txt). On 2026-11-01 the CLI refused
# GPT-6.1 Terra (model_not_found), so a run that fails within a minute moves on to GPT-6 Sol.
# CANOPY_CODEX_MODEL picks one model and skips the list.
$ErrorActionPreference = "Stop"
$prompt = "You are taking over the Canopy native Rust rewrite from Claude, whose usage quota ran out; Claude takes over again when its quota resets, so leave everything in a state another agent can pick up. " +
  "Step 1: read D:\projects\canopy\HANDOFF.md first, then D:\projects\canopy\AGENTS.md. HANDOFF.md says what is done, the next steps in order, how to build and test, and the rules (branches, never merge into main, never edit the OneDrive StreamCuts worktree). " +
  "Step 2: set the Active agent line in HANDOFF.md to Codex and commit that. " +
  "Step 3: work through the next steps in order. Build and test after each one, and commit after each step that builds, on the branches HANDOFF.md names, with a clear message. " +
  "Step 4: before you stop for any reason, update HANDOFF.md (done, in progress, next steps, anything surprising), set the Active agent line to none, and commit."
Set-Location D:\projects\canopy
$toolchain = Join-Path $env:LOCALAPPDATA "canopy-toolchain"
$thread = $env:CANOPY_CODEX_THREAD
$threadFile = Join-Path $toolchain "codex-thread.txt"
if (-not $thread -and (Test-Path $threadFile)) { $thread = (Get-Content $threadFile -Raw).Trim() }
if ($thread) {
  Write-Host "Handing over to the ChatGPT app's Codex session $thread"
  codex queue --thread $thread --message $prompt
  if ($LASTEXITCODE -eq 0) {
    Write-Host "Codex is working in the ChatGPT app. HANDOFF.md says Active agent: Codex until it hands back."
    return
  }
  Write-Host "Queueing into the app session failed; running Codex here instead"
}
$last = Join-Path $toolchain "codex-last.txt"
$models = if ($env:CANOPY_CODEX_MODEL) { @($env:CANOPY_CODEX_MODEL) } else { @("gpt-6.1-terra", "gpt-6-sol") }
foreach ($model in $models) {
  Write-Host "Starting Codex with $model"
  $started = Get-Date
  codex exec -m $model --dangerously-bypass-approvals-and-sandbox -C D:\projects\canopy --add-dir D:\projects\streamcuts -o $last $prompt
  if ($LASTEXITCODE -eq 0 -or ((Get-Date) - $started).TotalSeconds -gt 60) { break }
  Write-Host "Codex stopped at once with $model; trying the next model"
}

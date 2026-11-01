# Hands the Canopy native rewrite to Codex (GPT-6.1 Terra, full access) in this terminal.
# Codex reads HANDOFF.md, continues from the last commit, and keeps HANDOFF.md current, so Claude can take over again.
# The prompt is one line without quote marks: Windows PowerShell mangles both when it passes them to codex.
# codex exec runs unattended (no folder-trust prompt, no waiting for input) and exits when Codex stops,
# so the tab going idle means Codex is done. Its last message lands in %LOCALAPPDATA%\canopy-toolchain\codex-last.txt.
$ErrorActionPreference = "Stop"
$prompt = "You are taking over the Canopy native Rust rewrite from Claude, whose usage quota ran out; Claude takes over again when its quota resets, so leave everything in a state another agent can pick up. " +
  "Step 1: read D:\projects\canopy\HANDOFF.md first, then D:\projects\canopy\AGENTS.md. HANDOFF.md says what is done, the next steps in order, how to build and test, and the rules (branches, never merge into main, never edit the OneDrive StreamCuts worktree). " +
  "Step 2: set the Active agent line in HANDOFF.md to Codex and commit that. " +
  "Step 3: work through the next steps in order. Build and test after each one, and commit after each step that builds, on the branches HANDOFF.md names, with a clear message. " +
  "Step 4: before you stop for any reason, update HANDOFF.md (done, in progress, next steps, anything surprising), set the Active agent line to none, and commit."
Set-Location D:\projects\canopy
$last = Join-Path $env:LOCALAPPDATA "canopy-toolchain\codex-last.txt"
codex exec -m gpt-6.1-terra --dangerously-bypass-approvals-and-sandbox -C D:\projects\canopy --add-dir D:\projects\streamcuts -o $last $prompt

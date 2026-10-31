# Hands the Canopy native rewrite to Codex (GPT-6.1 Terra, full access) in this terminal.
# Codex reads HANDOFF.md, continues from the last commit, and keeps HANDOFF.md current, so Claude can take over again.
$ErrorActionPreference = "Stop"
$prompt = @'
You are taking over the Canopy native Rust rewrite from Claude, whose usage quota ran out. Claude will take over again when its quota resets, so leave everything in a state another agent can pick up.

1. Read D:\projects\canopy\HANDOFF.md first, then D:\projects\canopy\AGENTS.md. HANDOFF.md says what is done, what is in progress, the next steps in order, how to build and test, and the rules (branches, never merge into main, never edit the OneDrive StreamCuts worktree).
2. Set "Active agent" in HANDOFF.md to Codex and commit that.
3. Work through the next steps in order. Build and test after each one. Commit after each step that builds (on the branches HANDOFF.md names), with a clear message.
4. Before you stop, for any reason, update HANDOFF.md (done, in progress, next steps, anything surprising), set "Active agent" to none, and commit.
'@
Set-Location D:\projects\canopy
codex -m gpt-6.1-terra --dangerously-bypass-approvals-and-sandbox -C D:\projects\canopy $prompt

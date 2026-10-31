# Working on Canopy

Canopy runs Forma, Folio and StreamCuts in one window. Work on these branches:

- This repo: `dev`. Never merge into `main`; the owner merges after testing.
- Forma (`D:\projects\forma-studio`) and Folio (`D:\projects\folio-carousel-studio`): `canopy`. Run `git status` before switching, and ask before moving uncommitted changes.
- StreamCuts: `canopy` in `D:\projects\streamcuts` (its native Rust rewrite lives in `native\`). Never edit the OneDrive worktree (`...\ChatGPT\StreamCuts\p3-integration`): it's Codex's, with uncommitted work.
- The native Rust rewrite (egui) is in progress: read `HANDOFF.md` first, and keep it current when you stop.

Checks before committing:

- `npm test`: unit tests, then `tests/workspace.cjs`, which drives a real space with the Forma and Folio checkouts (bundle Forma first with `npm run bundle` there).
- After changing Forma's or Folio's `app-host.cjs`, also run that app's own `npm test`: the standalone app uses the same module.
- `npx electron tests/streamcuts.cjs` when StreamCuts hosting changes (slow; it starts StreamCuts' servers).
- `npm run build:win`, then `dist\Canopy-*-portable.exe --smoke-test`, before replacing `D:\Software\Canopy.exe`.

How apps plug in, what a space folder holds and how files move: `docs/APP_CONTRACT.md`. Code layout: `README.md`.

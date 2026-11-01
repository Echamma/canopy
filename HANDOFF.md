# Canopy native rewrite: handoff

Active agent: Claude (since 2026-11-01 02:40 local; Codex never started: the interactive TUI stopped at its folder-trust prompt, so the hand-off script now uses `codex exec`)

Claude and Codex take turns on this. Whoever stops updates this file (Done, In progress, Next steps), sets **Active agent** to `none`, and commits. Whoever starts reads it, sets **Active agent** to themselves, and commits.

## Goal

The owner asked for everything on the Canopy branches to be rewritten in Rust, apps included, with **native UI** (no web views), starting with **Canopy and StreamCuts**. Forma and Folio follow later. The Electron versions keep working meanwhile: `D:\Software\Canopy.exe`, `Forma.exe` and the Folio exe are not touched until the native ones reach parity.

## Rules

- Branches: Canopy repo `D:\projects\canopy` → `dev`. StreamCuts native → `D:\projects\streamcuts` (a fresh clone of Echamma/StreamCuts) on branch `canopy`. Forma and Folio → their `canopy` branches. Never merge into `main`; the owner merges after testing.
- Never edit `C:\Users\elias\OneDrive\Documents\ChatGPT\StreamCuts\p3-integration`: it is a Codex worktree with uncommitted work. Read it for reference only.
- Commit after every step that builds. Keep this file current.

## Toolchain (read this before building)

- Rust 1.96, default toolchain `stable-x86_64-pc-windows-gnu`. There is **no C compiler and no MSVC linker** on this machine, so every dependency must be pure Rust (no `cc`/bindgen crates: avoid ring, openssl, zstd-sys, libz-sys, ffmpeg-sys, …). eframe 0.37 (wgpu renderer by default), egui 0.37, cpal 0.18, rfd 0.17, image 0.25, serde, dirs 7 all compile.
- Recent `windows-*` crates use raw-dylib and need a `dlltool`. The GNU toolchain has none on PATH; its bundled GNU dlltool (`...\bin\self-contained\dlltool.exe`) fails because it needs `as.exe`, which isn't shipped. What works: `native\scripts\setup-toolchain.ps1` copies LLVM's `llvm-ar.exe` (from the `llvm-tools` component) to `%LOCALAPPDATA%\canopy-toolchain\dlltool.exe`; both repos' `native\.cargo\config.toml` point `-C dlltool` at it.
- Solved: with GNU `ld`, binaries that pull in eframe crashed at startup (`STATUS_ACCESS_VIOLATION`). Both `.cargo\config.toml` files therefore also link with **rust-lld** (`-C linker=rust-lld -C linker-flavor=ld.lld`); everything builds, runs and tests with that. Don't change it back.
- A full rebuild from clean takes about 5 minutes; `cargo build` after that is quick.
- egui 0.37 API notes: `eframe::App` has `fn ui(&mut self, ui: &mut egui::Ui, frame)`; panels are `egui::Panel::left(id).show(ui, ..)` / `CentralPanel::default().show(ui, ..)`; `Painter::rect_stroke` takes a `StrokeKind`; `DroppedFile` is a trait object with `.path() -> &Path`; `Id::new` is deprecated (use `Id::unique` / `IdSalt::new`); `on_exit(&mut self)` takes no GL argument.
- FFmpeg 8.1 (gyan full build, has libx264 and aac) and FFprobe are on PATH (winget). Media decode and export run them as child processes: no linking.
- **Starting the static FFmpeg build takes ~0.6 s** on this machine (a 242 MB exe); the shared build (also on PATH, `Gyan.FFmpeg.Shared`) takes ~25 ms. `ffmpeg.rs` prefers a shared build when PATH has both. Never start FFmpeg on the UI thread (use `Background`): with a static build that freezes the editor.
- Screenshots of a window that is playing video can mix content from different frames (GDI capture of a GPU window): to check timing, log from the app instead. The editor redraws at the monitor's rate (165 Hz here, AutoVsync).

## Layout

```
D:\projects\canopy\native\            Cargo workspace (Canopy repo, branch dev)
  crates\canopy-core\                 spaces + shared Files: same on-disk format as Electron Canopy (lib/spaces.cjs, lib/files.cjs)
  crates\canopy-ui\                   Canopy Night theme for egui, leaf shapes, widgets, the CanopyApp trait, drag payloads
  crates\canopy\                      the shell (bin "canopy")
D:\projects\streamcuts\native\        StreamCuts native (StreamCuts repo, branch canopy): lib + bin "streamcuts"
```

The shell depends on `streamcuts` by path (`../../../../streamcuts/native`); StreamCuts depends on `canopy-ui` by path (`../../canopy/native/crates/canopy-ui`). Both checkouts must sit side by side in `D:\projects`.

Apps are libraries that implement `canopy_ui::CanopyApp`. Canopy shows them as panes in one egui window (one process, no IPC). Each app also has its own binary that runs it alone, which keeps the "open it from its own exe" requirement.

Spaces are shared with Electron Canopy: `%APPDATA%\Canopy\spaces.json` and the same space folders (`canopy-space.json`, `Files\`, `Folio\`, `.canopy\<app>\`). Native StreamCuts keeps its projects in `<space>\StreamCuts\*.streamcuts.json`.

## Done

- Electron Canopy with Forma, Folio and StreamCuts (see README.md); Forma/Folio `canopy` branches pushed.
- `native/crates/canopy-core`: spaces registry and folders, shared Files (list, unique names, copy, write, atomic writes), same JSON as Electron Canopy. `cargo test -p canopy-core`: 11 pass.
- `native/crates/canopy-ui` (compiles): Canopy Night theme (`theme.rs`: colors, moods, accents, Nunito/Nunito Sans TTFs in `fonts/` converted from the bundled WOFF2), `paint.rs` (glowing dotted ground, leaf shapes, 26 line icons), `widgets.rs` (pill, icon button, primary/quiet buttons, two-line rows, sizes), `app.rs` (`CanopyApp`, `Host`, `AppDescriptor`, `Place`, `SharedFile`, `DraggedFiles`, `Document`, `Receipt`), `standalone.rs` (`run_standalone`: an app alone in its own window, with OS file drops, toasts, save on close).
- StreamCuts `native/` (branch `canopy` in D:\projects\streamcuts), 15 tests pass, including real FFmpeg probing/decoding and a two-track render:
  - `project.rs` model + edit rules + undo; `ffmpeg.rs` probe, stills, thumbnails, `FrameStream` (playback video), `SoundStream` (playback audio, not wired up yet); `export.rs` one-run filter graph + runner; `storage.rs` `.streamcuts.json` projects; `worker.rs` background probe/thumbnail/still jobs; `preview.rs` stills while paused, frame stream while playing, wall clock; `timeline.rs` ruler, tracks with mute/hide, move/trim/select, snapping, zoom, scroll, drops; `app.rs` the editor (toolbar with name, undo/redo, Import, 16:9/9:16/1:1, Projects, Export; media bin; preview + transport; timeline bar with split/delete/snap/add track/zoom; Space, S, Delete, Ctrl+Z, arrows, Home/End; autosave; export dialog; receive files at the playhead).
  - `cargo run` in `native\` opens it alone (`streamcuts.exe`, projects in `Documents\StreamCuts Projects`, exports in `Documents\StreamCuts Exports`; `CANOPY_APP_HOME=<folder>` redirects both). Checked by screenshot: an 8 s clip passed on the command line is probed, thumbnailed, placed and previewed.
- Native shell `native/crates/canopy` (`cargo run -p canopy`; `canopy <space folder>` opens that space; `CANOPY_PROFILE` and `CANOPY_SPACES_DIR` redirect the registry and new spaces for tests): Where to? (tiles, New space dialog with color and folder, open a space folder, right-click show/remove), a space (top bar with space pill, app tabs, toast + Send to, split toggle, clock; sidebar with search, Projects, Files with add/reveal, drag rows onto panes, double-click / right-click Send to; dock; launcher; one or two panes with header split/close). StreamCuts runs natively in its pane; Forma and Folio panes open their exe. Checked by screenshot.
- StreamCuts plays sound (`audio.rs`): one FFmpeg `SoundStream` per audible clip within 2 s of the playhead, mixed by timeline sample on cpal's output thread; the samples played are the playback clock (`Audio::now`), so the picture follows the sound. Playback pre-rolls (holds the clock until the first sound arrives, at most 2 s), stops if the editor isn't drawn for 400 ms (hidden pane, minimized), and restarts when an edit changes what's audible. No output device: silent, on the wall clock, with a toast. `cargo test -- --include-ignored` also runs a test that plays a near-silent tone through the real output.
- FFmpeg streams (`FrameStream`, `SoundStream`) start on a background thread (`Background` in `ffmpeg.rs`), and the preview starts the next clip's stream 1 s before a cut. Stills and streams are frame-exact (`seek_to` asks a millisecond early; streams start on a frame boundary), checked by the FFmpeg test.
- `native\scripts\window-shot.ps1 -Exe <exe> [-Out <png>] [-AppArgs <args>] [-AppHome <folder>] [-Steps "key: ", "wait:1500", "click:x,y", "drag:x1,y1,x2,y2", "shot:<png>", ...]` launches an app, drives it (input only while its window is in front), screenshots it and closes it.

## In progress

- Nothing half-done. All work is committed.

## Next steps, in order

1. **Check the shell's interactions by running it** (`window-shot.ps1` or by hand): open StreamCuts in a pane, drag a sidebar file onto it, export (toast offers Send to), split view with a Forma placeholder, Ctrl+Tab, switching spaces saves projects, closing saves. Fix what doesn't work; there are no automated shell tests yet. Consider factoring shell state changes (show_app, set_split, send) into a struct you can unit-test without a window.
2. **Polish StreamCuts:** waveforms on audio clips (decode a low-rate envelope with FFmpeg on the worker), filmstrip thumbnails along video clips, a playhead you can drag on the ruler, ripple delete, clip volume in a small inspector, keyboard focus only for the focused pane (add `fn focused(&self) -> bool` to `Host`; the shell knows which pane is focused), `Id::new` deprecation warnings.
3. **Package:** `cargo build --release` for both; ship `canopy.exe` (native) next to the Electron one only when the owner says so. Don't replace `D:\Software\Canopy.exe` without asking.
4. **Scrubbing with a static FFmpeg** is slow (each paused still is a new FFmpeg process, ~0.6 s to start with a static build). If that matters, keep one FFmpeg per media file reading frames on demand, or decode stills from the nearest keyframe in a persistent process.
5. Then, StreamCuts parity, roughly in order of use: captions (whisper.cpp would need C; prefer calling the existing Python faster-whisper backend script or a pure-Rust whisper such as candle), transitions, text/titles, effects, the Boss AI clipping workflow (Gemini HTTP API), vertical shorts. Then Forma native, then Folio native.

## Build and test

```
cd D:\projects\canopy\native
cargo test
cargo run -p canopy
cd D:\projects\streamcuts\native
cargo run
```

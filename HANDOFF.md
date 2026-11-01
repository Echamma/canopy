# Canopy native rewrite: handoff

Active agent: none

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
- StreamCuts polish (2026-11-01): waveforms (`waveform.rs`: 100 peaks/s pyramid, decoded on its own thread, square-root scale), filmstrips (`filmstrip.rs`: up to 120 frames per video in one FFmpeg run, keyframes only past 2 min, one atlas texture), both handed to the timeline as `Looks`; clip volume slider in the timeline bar when one clip with sound is selected; ripple delete (Shift+Delete, `Project::ripple_delete`); shortcuts only in the focused pane (`Host::focused`); no deprecation warnings. Dragging on the ruler already moved the playhead.
- Captions (2026-11-01): `transcribe.rs` cuts a clip's sound with FFmpeg and runs the web backend's faster-whisper script (`backend/long-to-short/python/transcribe.py`, found above the binary or via `STREAMCUTS_TRANSCRIBE`; `STREAMCUTS_PYTHON`); `Project.captions` in timeline time (they don't follow clips that move); Captions button + "Making captions…" + a text field for the selected caption in the timeline bar; a Captions lane under the tracks; `captions.rs` draws them on the preview and writes the ASS file exports burn in (FFmpeg runs in a temp folder with the ASS and the font; libass needs the size ×1.385 to match egui, measured). Checked end to end with a text-to-speech clip. faster-whisper's `small`, `medium` and `tiny` models are cached on this machine.
- Fades, framing and titles (2026-11-01): clips have `fade_in`/`fade_out` (export: alpha `fade` + `afade`; preview darkens; the mixer applies the envelope) and `Framing::Fit`/`Fill { x, y }` (one FFmpeg filter, `Framing::filter`, used by export, stills and frame streams, so preview = export); a "Fades & framing" menu for the selected clip. `Project.titles` (text at a height and size, drag to move on a Titles lane, "Size & place" menu; burned in through the same ASS file as captions with `\pos`/`\fs`; a title past the clips makes an end card). Checked by export frames against the preview.
- Color and speed (2026-11-01): `Clip.grade` (`Grade`: brightness/contrast/saturation → FFmpeg `eq`) and `Clip.speed` (0.25-4×; `source_time` scales by it, `Project::set_speed` keeps the source stretch and stops at the next clip, export uses `setpts` + an `atempo` chain from `ffmpeg::atempo`, preview streams and playback sound play at it, transcription maps through it). Framing and grade travel together as `Picture`, whose `filter` export, stills and frame streams all use. All of a clip's settings are in its "Clip" menu in the timeline bar.
- Also 2026-11-01: an "Even loudness" toggle beside Export (`Project.even_loudness`: one pass of `loudnorm` to -14 LUFS on the final mix); missing source files are marked in Media ("click to find it", `find_media`), named on the preview, and stop Export with a clear message; captions record their clip (`Caption.clip`) and move with it on move, ripple delete and split (captions without one go by the clip they sit inside).
- Titles trim by their edges in the Titles lane; unused media can be removed (right-click in Media); dragging up or down on the preview moves the selected title, or with a caption selected, where captions sit (`Project.caption_bottom`).
- Scrubbing shows a filmstrip stand-in while a slow still loads (`Preview::waiting`, `SLOW_STILL`).
- Waveforms and filmstrips are cached on disk (`cache.rs`: `<data_dir>/looks/<hash of path, size, mtime>.wave|.strip`), so reopening a project skips decoding.
- Release builds (`[profile.release]` strip + thin LTO in both workspaces): canopy.exe and streamcuts.exe about 24-25 MB each, ~4 minutes each from scratch. The release Canopy was checked by running it on a scratch space. Nothing was copied to `D:\Software`.
- Long files (2026-11-01, a 1-hour 320x180 file with sound, shared FFmpeg): probe 95 ms, a still at 45:00 115 ms, filmstrip 1.1 s, waveform 1.8 s (both on their own threads), so a 12-hour stream is ~13 s and ~22 s.
- The shell was checked by running it (2026-11-01, `window-shot.ps1` against a throwaway space with `CANOPY_PROFILE`/`CANOPY_SPACES_DIR` in the scratchpad): StreamCuts in a pane, a sidebar file dragged onto its timeline (was broken: egui's `take_payload` clears a payload of another type, so ask with `has_payload_of_type` first), export into the space's Files, split view (StreamCuts' toolbar now folds below 780 px), Ctrl+1/Ctrl+3, playback pausing when its pane is hidden, the sidebar's hint (no longer on the dock), centered windows. `cargo test -p canopy` has unit tests for showing apps, split and send.
- `native\scripts\window-shot.ps1 -Exe <exe> [-Out <png>] [-AppArgs <args>] [-AppHome <folder>] [-Log <stderr file>] [-Steps "key: ", "wait:1500", "click:x,y", "drag:x1,y1,x2,y2", "down:x,y", "move:x,y", "up", "shot:<png>", ...]` launches an app, drives it (input only while its window is in front), screenshots it and closes it. Coordinates are pixels in the window's visible frame, the same as in its screenshots. Never point it at the real spaces: set `CANOPY_PROFILE` and `CANOPY_SPACES_DIR` (shell) or `-AppHome` (standalone) to a scratch folder.
- Codex's 2026-11-01 native work, committed on the named branches and pushed to `origin`:
  - StreamCuts `canopy`: latest-only scrub still jobs and stale-result rejection (`c3b94c6`); Boss AI with full-source 10-minute transcription chunks, Gemini planning through `curl.exe`, a suggestion picker and 9:16 project creation (`3af2e2b`); adjacent same-track crossfades using the outgoing clip's unused source tail in preview, audio and export (`74fada4`); shared blur/sharpen picture effects (`4b021af`). `STREAMCUTS_BOSS_RESPONSE=<JSON file>` supplies a recorded Gemini response for testing. Gemini key is read from `GEMINI_API_KEY` and passed to curl over stdin, not command arguments. A real crossfade FFmpeg render and local curl HTTP test passed. No live Gemini call was possible: this machine has no key.
  - Canopy `dev`: native Forma is a path-dependent app in the shell (`032cd04`), with image and renderer dependencies locked (`fc3d8a6`, `4a7f2fe`). It runs in a pane instead of launching the Electron exe.
  - Forma `canopy`: native Rust scene model preserving unknown JSON fields (`aa571a9`), standalone egui editor (`701261a`), compatible image library and preview (`145d1bb`), color/rotation/disappearance/linked-text preview (`abd3bc6`), UTF-8 BOM import (`8f7d628`), undo/redo and layer actions (`79f95dd`), scene settings and animation presets (`dcd1087`), shape/picture PNG export (`24f0104`), shape/picture silent MP4 export through FFmpeg with progress and cancel (`4c4aa7c`), and free text layer creation/editing (`39f8cdb`). The standalone editor and a saved shape fixture were checked by screenshot. MP4 export was encoded and decoded by real FFmpeg in a test.
  - Final checks: Forma `cargo test --offline` (11 pass) and `cargo build --offline` from `D:\projects\forma-studio\native` with `CARGO_TARGET_DIR=D:\projects\canopy\native\target`; Canopy workspace `cargo test --offline` (3 shell + 11 core pass) and `cargo build --offline`. These were rerun after the last Forma change. The Electron code was not changed, so its npm checks were not rerun. No new release build was made or installed.

## In progress

- No uncommitted code. Forma remains short of Electron feature parity, as listed below.

## Next steps, in order

1. **Ship when the owner says so:** the release exes build (see Done). Don't put them in `D:\Software` or replace `D:\Software\Canopy.exe` without asking. StreamCuts needs FFmpeg on PATH (or `STREAMCUTS_FFMPEG`), and Python with faster-whisper for captions; a shipped StreamCuts should carry `transcribe.py` next to it (`transcribe::script` looks in the exe's parent folders).
2. **Validate Boss AI live** when the owner sets `GEMINI_API_KEY` in the process environment. Use a short test VOD, inspect suggestions and create a project; keep the key out of logs. The recorded-response and local HTTP paths already pass. The default model in native code follows the web backend (`GEMINI_MODEL`, default `gemini-3.5-flash`).
3. **Finish native Forma parity before starting Folio.** The editor handles shapes, pictures, free text, keyframes, basic presets and compatible project JSON. Complete text rendering in PNG/MP4 (currently exports return an explicit error for text), SVG and video layers, audio, `.forma` archives, and the remaining Electron animation/editing tools. Existing unsupported fields survive load/save; do not silently omit visible layers in export. Its standalone default home is `%APPDATA%\forma-native`, while in Canopy it uses the existing space `.canopy\forma\projects`. Build/test the Forma crate and the Canopy workspace after each step, then commit on Forma `canopy` or Canopy `dev` as applicable.
4. **Build Folio native** on Folio `canopy` and host it in Canopy `dev`, following `docs\APP_CONTRACT.md`. Verify shared-space round trips and a standalone window.
5. **Shipping remains owner-gated.** New Rust changes have debug builds and tests but no fresh release build. Before replacing `D:\Software\Canopy.exe`, build the release apps and have the owner test/approve the replacement. Never merge the branches into `main` here.

## Build and test

```
cd D:\projects\canopy\native
cargo test
cargo run -p canopy
cd D:\projects\streamcuts\native
cargo run
```

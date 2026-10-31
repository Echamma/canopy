# Canopy native rewrite: handoff

Active agent: Claude

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
- Known problem being worked on: with that dlltool and the default GNU `ld`, a small crate using `windows-result` links and runs, but the StreamCuts lib **test binary crashed at startup** (`STATUS_ACCESS_VIOLATION`, even `--list`), probably a data import in eframe's dependency tree mislinked. The config now also links with `rust-lld` (`-C linker=rust-lld -C linker-flavor=ld.lld`). If that still crashes or doesn't link: next options are (1) keep GNU ld but try `-C link-arg=-Wl,--disable-runtime-pseudo-reloc`, (2) install MinGW-w64 (`winget install -e --id BrechtSanders.WinLibs.POSIX.UCRT`) and use its `dlltool`/`as`/`ld`, (3) the MSVC toolchain, which needs Visual Studio Build Tools.
- egui 0.37 API notes: `eframe::App` has `fn ui(&mut self, ui: &mut egui::Ui, frame)`; panels are `egui::Panel::left(id).show(ui, ..)` / `CentralPanel::default().show(ui, ..)`; `Painter::rect_stroke` takes a `StrokeKind`; `DroppedFile` is a trait object with `.path() -> &Path`; `Id::new` is deprecated (use `Id::unique` / `IdSalt::new`); `on_exit(&mut self)` takes no GL argument.
- FFmpeg 8.1 (gyan full build, has libx264 and aac) and FFprobe are on PATH (winget). Media decode and export run them as child processes: no linking.

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
- StreamCuts `native/` (branch `canopy` in D:\projects\streamcuts): `project.rs` (model + all edit rules + undo, 9 tests), `ffmpeg.rs` (probe, still frames, thumbnails, `FrameStream` for playback video, `SoundStream` for playback audio), `export.rs` (one-run FFmpeg filter graph + background runner with progress and cancel, 3 tests). Tests are written but haven't run yet because of the startup crash above.

## In progress

- Confirming the `rust-lld` link fixes the test binary crash (`cd D:\projects\streamcuts\native; cargo test`).

## Next steps, in order

1. Make `cargo test` pass in `D:\projects\streamcuts\native` (see Toolchain). Then run `cargo test` in `D:\projects\canopy\native`.
2. StreamCuts UI (`native/src/editor/` or similar), then `pub static DESCRIPTOR: AppDescriptor` in lib.rs and `main.rs` = `canopy_ui::run_standalone(&streamcuts::DESCRIPTOR)`. Projects are `<projects_dir>\<name>--<id8>.streamcuts.json`, autosaved ~1 s after each edit; `documents()` lists them. Layout: toolbar (project name, undo/redo, Import, 16:9 / 9:16 / 1:1, Export as the primary button); media bin on the left (thumbnails from `ffmpeg::thumbnail` on a worker thread, drag to the timeline with an egui DnD payload); preview on the right (aspect-fit, `ffmpeg::still` while paused and scrubbing, `FrameStream` while playing; play/pause, step, timecode); timeline below (ruler, track headers with mute/hide, clips with thumbnails, playhead; click/drag ruler to scrub, drag clips to move (`Project::move_clip`), drag edges to trim, S split (`split` selected or `split_all`), Delete, Ctrl+Z/Ctrl+Shift+Z, Space play, Ctrl+wheel zoom, wheel scroll, snapping to `snap_points` + playhead within 8 px). Audio: cpal output stream; mix `SoundStream`s of `audible_clips()` started at play time, with the audio clock driving the playhead. Export dialog: runs `export::start` into `place.exports_dir`, progress bar, cancel, then `host.exported(path)`. `receive()`: probe each file, add media, place at the playhead.
3. Native shell (`native/crates/canopy/src`): Where to? (spaces, new space, open folder), space view (top bar with app tabs, sidebar Projects + Files with search, add, drag out, right-click Send to), one or two panes, Ctrl+Tab / Ctrl+1-3, toasts with Send to, files refresh. Forma and Folio panes say they aren't native yet and offer to open their current exe.
   The shell hosts apps through `AppDescriptor` (StreamCuts: `streamcuts::DESCRIPTOR`); Forma and Folio get placeholder panes that open `D:\Software\Forma.exe` / `D:\Software\Folio Carousel Studio.exe` until they're native. Sidebar file rows are egui drag sources with a `canopy_ui::DraggedFiles` payload; a drop on a pane that the app didn't take goes to `app.receive`. `Host::exported` → toast with Send to buttons. Match the Electron shell's look: its source is `D:\projects\canopy\shell\` (`shell.css` has every size and spacing), `npx electron scripts/screenshot.cjs <folder>` captures it, and the tokens are already in `canopy-ui/src/theme.rs`.
4. Tests and a smoke run of both binaries (`cargo run -p canopy`, `cargo run` in StreamCuts native); commit; update this file.
5. Then, StreamCuts parity, roughly in order of use: captions (whisper.cpp would need C; prefer calling the existing Python faster-whisper backend script or a pure-Rust whisper such as candle), transitions, text/titles, effects, the Boss AI clipping workflow (Gemini HTTP API), vertical shorts. Then Forma native, then Folio native.

## Build and test

```
cd D:\projects\canopy\native
cargo test
cargo run -p canopy
cd D:\projects\streamcuts\native
cargo run
```

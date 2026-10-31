# How an app joins Canopy

Canopy shows apps side by side in one window, inside a **space**. A space is a folder you can see on disk; every app keeps its work for that space inside it, and all apps share the space's `Files` folder. Each app still runs on its own from its own exe. This page is the contract an app follows to do both.

## The space folder

```
<space>/
  canopy-space.json   {"version":1,"id":"…","name":"Launch week","color":"clay","createdAt":…}
  Files/              shared files: imports come from here, exports land here
  Folio/              Folio's carousels for this space (*.folio.json)
  .canopy/forma/      Forma's projects, media library and recovery copy for this space
  .canopy/folio/      Folio's recovery copy for this space
  .canopy/canopy/     which apps were open, so the space reopens the way you left it
```

Spaces are listed in `%APPDATA%\Canopy\spaces.json`. Taking a space off the list leaves its folder alone, and **Open a space folder** brings it back (on this computer or another one).

## Electron apps: `app-host.cjs`

Forma and Folio are Electron apps. Everything their main process does for the editor lives in `app-host.cjs` at the app's root. The standalone `main.cjs` and Canopy both call it, so there is one copy of the logic:

```js
module.exports = {
  id: 'forma', name: 'Forma', accent: 'clay',
  schemes: [/* custom schemes, registered by whoever hosts the app before it is ready */],
  check: () => null,              // optional: a message when this checkout can't run (Forma: bundle missing)
  documents: async ({dataDir, canopy}) => [{id, name, savedAt, detail}],  // optional: projects, for Canopy's sidebar
  create(context) { return host; }
};
```

`create(context)` receives:

| Field | Standalone | Inside Canopy |
| --- | --- | --- |
| `ipc` | `ipcMain` | a router scope: same API, but calls are routed to the page that made them, so two apps can both register `load-recovery` |
| `dataDir` | the app's `userData` | `<space>/.canopy/<id>` |
| `session` | `session.defaultSession` | the app's own session for this space (`persist:canopy-<space>-<id>`) |
| `webContents()`, `window()` | the app's window | the app's view, Canopy's window |
| `documentsDir`, `exportDirectory`, `discoveryFile` | as before | Documents, `<space>/Files`, Forma's usual MCP discovery file |
| `canopy` | `null` | `{space: {id, name, dir}, projectsDir: <space>/Folio, filesDir: <space>/Files, saveFile({name, data})}` |

and returns:

| Field | Purpose |
| --- | --- |
| `preload`, `entry`, `additionalArguments` | how to load the editor. Inside Canopy, `additionalArguments` carries `--canopy={"space":"…"}`, which the preload exposes as `desktop.canopy` / `formaDesktop.canopy` |
| `checkpoint()` | saves the open work; Canopy calls it before switching spaces or closing |
| `receive(files)` | takes `[{path, name, type, size}]` from the space and returns `{accepted, skipped}` |
| `listDocuments()`, `openDocument(id)` | the sidebar's Projects list, and opening one of them |
| `dispose()` | removes IPC handlers and protocol handlers, stops servers |

Inside Canopy an app should not show its own save dialogs for exports: hand the bytes to `canopy.saveFile`, or let a download happen. Canopy saves every download from an app's session into `Files`.

## Web apps (StreamCuts)

StreamCuts runs as its local web app. Canopy starts it like `StreamCutsLauncher` does (backend on :4000, editor on :3000), or uses copies already running, and stops only what it started. Its page gets `preloads/web-app.cjs`, which:

- sets `window.canopy = {hosted: true, app, space, saveFile(name, blob)}`;
- delivers files sent from the space as a **paste**: each file is fetched from `canopy-file://files/<name>` and dispatched as a `paste` event, which StreamCuts imports and places at the playhead. If nothing takes the paste (no project open), Canopy says so.

Downloads from StreamCuts (clip exports) land in `Files`. Full video exports use StreamCuts' own save picker, which streams long videos to disk; save them into the space's `Files` folder to share them.

## Moving files between apps

- **Drag** a file from the sidebar onto any app. It arrives as a normal file drop, so every app's own drop handling works.
- **Double-click** a file to send it to the app on screen (or the first app that can open it), or **right-click** for *Send to Forma / Folio / StreamCuts*.
- **Exports** from any app land in `Files`, and show up in the sidebar for the next app.

What each app takes:

| App | Takes |
| --- | --- |
| Forma | PNG, JPEG, WebP, SVG, MP4, WebM and Ogg video (as layers), `.forma` and Forma `.json` projects |
| Folio | PNG, JPEG and WebP under 15 MB (as movable image layers), `.folio.json` projects |
| StreamCuts | pictures, video and audio (added at the playhead of the open project) |

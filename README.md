# Canopy

One window for Forma, Folio and StreamCuts. A **space** is a project that holds your work from all three apps and the files they share. Switch between the apps without losing your place, show two side by side, and move files from one to the next.

Each app still opens on its own from its own exe. Inside Canopy they keep their work per space; on their own they keep it where they always have.

## Use

- **Where to?** lists your spaces. **New space** asks for a name and a color and makes a folder under `Documents\Canopy` (or wherever you choose). **Open a space folder** (the folder button) adds one made elsewhere. Right-click a space to show it in Explorer or remove it from the list (its folder stays).
- In a space, the top bar has a tab for each app. Click one to open it; a dot shows it's running. **Ctrl+Tab** and **Ctrl+Shift+Tab** move between apps, and **Ctrl+1**, **Ctrl+2** and **Ctrl+3** jump to Forma, Folio and StreamCuts. Apps keep running in the background while you switch.
- The split button (top right, or in a pane's header) shows two apps side by side. Opening a third replaces the one you used least recently.
- The sidebar lists the space's **Projects** (Forma animations and Folio carousels; click one to open it) and **Files**.
  - Drag a file onto an app to import it there, double-click to send it to the app on screen, or right-click for *Send to…*, *Show in folder* and *Move to Recycle Bin*.
  - Drop files from Explorer onto the sidebar, or use **+**, to add them to the space.
  - Exports from Forma and Folio, and downloads from StreamCuts, land in Files. The message in the top bar then offers **Send to** the other apps that can open the file, so a Folio slide reaches Forma or StreamCuts in one click.
  - To bring in work from the standalone apps, export it there (Forma's **Project file (.forma)**, or a Folio `.folio.json`) and add it to the space's Files, then send it to the app.
- Canopy saves every app's work when you switch spaces or close it, and reopens a space with the apps you had open.

StreamCuts starts the way `StreamCuts.exe` does (its backend on port 4000, its editor on port 3000). If it's already running, Canopy uses it. The first start of a checkout without a production build runs the dev server, which takes a minute or two to compile.

## Where the apps come from

Running from source, Canopy uses the checkouts on this computer:

| App | Folder | Branch with Canopy support |
| --- | --- | --- |
| Forma | `D:\projects\forma-studio` | `canopy` |
| Folio | `D:\projects\folio-carousel-studio` | `canopy` |
| StreamCuts | `%USERPROFILE%\OneDrive\Documents\ChatGPT\StreamCuts\p3-integration`, then `D:\projects\AppsToIntegrate\OpenCut` | no changes needed |

To use other folders, write `%APPDATA%\Canopy\apps.json`, for example `{"streamcuts": "D:\\projects\\StreamCuts"}`, or set `CANOPY_FORMA_DIR`, `CANOPY_FOLIO_DIR` or `STREAMCUTS_ROOT`. A packaged Canopy carries its own copies of Forma and Folio (see `npm run vendor`); StreamCuts always runs from its checkout.

## Develop

Node.js 22. `npm install`, then `npm start`. Forma's editor must be bundled once (`npm run bundle` in Forma).

- `npm test` runs the unit tests and `tests/workspace.cjs`, which opens a space with the real Forma and Folio and checks switching, split view, sending files, exports landing in Files, per-space data and saving on close.
- `npx electron tests/streamcuts.cjs` starts StreamCuts in a space, creates a project and sends it a picture. It is slow the first time.
- `npx electron scripts/screenshot.cjs <folder>` saves screenshots of the main screens.
- `npm run build:win` vendors Forma and Folio into `apps/` and builds `dist/Canopy-<version>-x64-portable.exe`. `Canopy.exe --smoke-test` checks a build: a throwaway space opens the Forma and Folio copies inside it.

## Known limits

- StreamCuts keeps its projects in its own browser storage (one per space), so they show in StreamCuts' project list, not in Canopy's sidebar. Its first-run welcome shows once in each space.
- StreamCuts' full video export uses its own save dialog, which streams long videos to disk; choose the space's Files folder there to share the video with the other apps. Clip exports land in Files on their own.
- An app shown in a narrow split pane gets cramped (Folio is laid out for 1100 px and up); use one pane for detailed work.

How an app plugs in, what a space folder holds and how files move between apps: [docs/APP_CONTRACT.md](docs/APP_CONTRACT.md).

## Layout

- `main.cjs`: window, shell IPC, saving on close
- `lib/workspace.cjs`: the open space, app views, panes, file hand-off, downloads
- `lib/spaces.cjs`, `lib/files.cjs`: space folders and the shared Files folder
- `lib/ipc-router.cjs`: lets several hosted apps register the same IPC channels
- `lib/apps.cjs`, `lib/streamcuts.cjs`: where the apps live, and running StreamCuts
- `preloads/`: the shell's API and the preload for web apps
- `shell/`: the Canopy Night interface

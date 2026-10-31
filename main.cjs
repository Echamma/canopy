const {app, BrowserWindow, ipcMain, dialog, protocol, Menu} = require('electron');
const path = require('node:path');
const {createIpcRouter} = require('./lib/ipc-router.cjs');
const {resolveApps} = require('./lib/apps.cjs');
const {createSpaces, COLORS} = require('./lib/spaces.cjs');
const {createWorkspace} = require('./lib/workspace.cjs');
const {safeFileName} = require('./lib/files.cjs');

const testMode = process.env.CANOPY_TEST === '1';
if (!testMode) {
  app.setPath('userData', path.join(app.getPath('appData'), 'Canopy'));
  if (!app.requestSingleInstanceLock()) { app.quit(); return; }
  app.on('second-instance', () => { if (window) { if (window.isMinimized()) window.restore(); window.show(); window.focus(); } });
}

// Each Electron app's host module is loaded up front: their custom schemes (Forma's forma-asset://) must be
// registered before Canopy is ready, together with Canopy's own canopy-file://.
const catalog = resolveApps({overridesFile: path.join(app.getPath('userData'), 'apps.json'), bundledDir: path.join(__dirname, 'apps'), preferBundled: app.isPackaged});
const modules = new Map();
for (const entry of catalog.filter(item => item.kind === 'electron' && item.dir)) {
  try { modules.set(entry.id, require(path.join(entry.dir, 'app-host.cjs'))); }
  catch (error) { entry.problem = `${entry.name} couldn't be loaded from ${entry.dir}: ${error.message}`; }
}
protocol.registerSchemesAsPrivileged([
  {scheme: 'canopy-file', privileges: {standard: true, secure: true, supportFetchAPI: true, stream: true, corsEnabled: true}},
  ...[...modules.values()].flatMap(module => module.schemes || [])
]);

let window, workspace, allowClose = false, closing = false;
const spaces = createSpaces({
  registryFile: path.join(app.getPath('userData'), 'spaces.json'),
  defaultRoot: process.env.CANOPY_SPACES_DIR || path.join(app.getPath('documents'), 'Canopy')
});
const send = (channel, value) => { if (window && !window.isDestroyed()) window.webContents.send(channel, value); };
const emit = (kind, value) => send('canopy:' + kind, value);

// Shell calls only count when they come from Canopy's own page.
const fromShell = event => !!window && !window.isDestroyed() && event.sender === window.webContents;
function shellHandle(channel, handler) {
  ipcMain.handle(channel, (event, ...args) => {
    if (!fromShell(event)) throw new Error('Unknown sender.');
    return handler(...args);
  });
}
function shellOn(channel, handler) {
  ipcMain.on(channel, (event, ...args) => { if (fromShell(event)) handler(event, ...args); });
}

function spaceMenu() {
  return spaces.list().then(list => {
    const current = workspace.currentSpace();
    Menu.buildFromTemplate([
      ...list.filter(space => space.available).map(space => ({label: space.name, type: 'radio', checked: space.id === current?.id, click: () => workspace.openSpace(space.id).catch(error => emit('toast', error.message))})),
      {type: 'separator'},
      {label: 'New space…', click: () => emit('command', 'new-space')},
      {label: 'Open a space folder…', click: () => addSpaceFolder().catch(error => emit('toast', error.message))},
      {label: 'Show this space in Explorer', enabled: !!current, click: () => workspace.revealSpace()},
      {type: 'separator'},
      {label: 'All spaces', click: () => workspace.closeSpace()}
    ]).popup({window});
  });
}
async function addSpaceFolder() {
  const result = await dialog.showOpenDialog(window, {title: 'Open a Canopy space', properties: ['openDirectory']});
  if (result.canceled || !result.filePaths[0]) return null;
  const space = await spaces.add(result.filePaths[0]);
  return workspace.openSpace(space.id);
}

function registerShell() {
  shellHandle('canopy:state', () => workspace.state());
  shellHandle('canopy:list-spaces', () => spaces.list());
  shellHandle('canopy:default-space-folder', () => process.env.CANOPY_SPACES_DIR || path.join(app.getPath('documents'), 'Canopy'));
  shellHandle('canopy:choose-space-folder', async () => {
    const result = await dialog.showOpenDialog(window, {title: 'Choose where to keep this space', properties: ['openDirectory', 'createDirectory']});
    return result.canceled ? null : result.filePaths[0] || null;
  });
  shellHandle('canopy:create-space', async ({name, color, parent} = {}) => {
    const space = await spaces.create({name, color: COLORS.includes(color) ? color : undefined, dir: parent ? path.join(parent, safeFileName(name)) : undefined});
    return workspace.openSpace(space.id);
  });
  shellHandle('canopy:add-space-folder', addSpaceFolder);
  shellHandle('canopy:open-space', id => workspace.openSpace(id));
  shellHandle('canopy:go-home', async () => {
    const failures = await workspace.closeSpace();
    if (failures.length) emit('toast', `Some work couldn't be saved: ${failures.join('; ')}`);
    return workspace.state();
  });
  shellHandle('canopy:space-menu', spaceMenu);
  shellHandle('canopy:reveal-space', () => workspace.revealSpace());
  shellHandle('canopy:open-app', id => workspace.openApp(id).then(() => true, error => { emit('toast', error.message); return false; }));
  shellHandle('canopy:focus-app', id => workspace.focusApp(id));
  shellHandle('canopy:close-app', id => workspace.closeApp(id));
  shellHandle('canopy:set-split', value => workspace.setSplit(value));
  shellHandle('canopy:list-files', () => workspace.listFiles());
  shellHandle('canopy:add-files', async () => {
    const result = await dialog.showOpenDialog(window, {title: 'Add files to this space', properties: ['openFile', 'multiSelections']});
    return result.canceled ? [] : workspace.addPaths(result.filePaths);
  });
  shellHandle('canopy:add-paths', paths => workspace.addPaths(Array.isArray(paths) ? paths.filter(item => typeof item === 'string' && path.isAbsolute(item)) : []));
  shellHandle('canopy:open-file', name => workspace.openFile(name));
  shellHandle('canopy:file-menu', name => workspace.fileMenu(name));
  shellHandle('canopy:list-documents', () => workspace.listDocuments());
  shellHandle('canopy:open-document', (appId, id) => workspace.openDocument(appId, id).catch(error => emit('toast', error.message)));
  shellOn('canopy:start-drag', (event, name) => { try { workspace.startDrag(event.sender, name); } catch (error) { emit('toast', error.message); } });
  shellOn('canopy:layout', (_, panes) => workspace.layout(panes));
  shellOn('canopy:overlay', (_, value) => workspace.setOverlay(value));
  // From the web-app preload inside StreamCuts.
  ipcMain.on('canopy:delivered', (event, result) => workspace.delivered(event, result));
  ipcMain.handle('canopy:web-save-file', (_, name, bytes) => workspace.saveFile({name, data: Buffer.from(bytes)}));
}

app.whenReady().then(() => {
  Menu.setApplicationMenu(null);
  window = new BrowserWindow({
    width: 1480, height: 940, minWidth: 1100, minHeight: 700, show: !testMode, backgroundColor: '#0C0A05', title: 'Canopy',
    icon: path.join(__dirname, 'assets', 'canopy.png'), autoHideMenuBar: true,
    webPreferences: {preload: path.join(__dirname, 'preloads', 'shell.cjs'), contextIsolation: true, nodeIntegration: false, sandbox: true}
  });
  workspace = createWorkspace({window, router: createIpcRouter(ipcMain), catalog, modules, spaces, emit, logDir: path.join(app.getPath('userData'), 'logs')});
  registerShell();
  if (testMode) global.__canopy = {workspace, spaces, window};
  workspace.watchShortcuts(window.webContents);
  window.webContents.setWindowOpenHandler(() => ({action: 'deny'}));
  window.webContents.on('will-navigate', event => event.preventDefault());
  // Every app saves before Canopy closes; if one can't, you choose whether to keep Canopy open.
  window.on('close', async event => {
    if (allowClose) return;
    event.preventDefault();
    if (closing) return;
    closing = true;
    try {
      const failures = await workspace.checkpointAll();
      if (failures.length) {
        const result = await dialog.showMessageBox(window, {type: 'warning', title: 'Save before closing', message: "Some work couldn't be saved automatically.", detail: failures.join('\n') + '\n\nChoose Cancel to keep Canopy open and save it in the app.', buttons: ['Cancel', 'Close anyway'], defaultId: 0, cancelId: 0});
        if (result.response === 0) return;
      }
      await workspace.shutdown();
      allowClose = true;
      window.close();
    } finally { closing = false; }
  });
  window.loadFile(path.join(__dirname, 'shell', 'index.html'));
});
app.on('window-all-closed', () => app.quit());

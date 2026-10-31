// The open space: which apps run in it, which of the (at most two) panes show them, and how files move between the
// apps and the space. Each app runs in its own WebContentsView and session, so Forma, Folio and StreamCuts keep their
// state while you switch between them, and every space keeps its own app data.
const {app, WebContentsView, session, shell, Menu, nativeImage} = require('electron');
const fs = require('node:fs');
const fsp = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');
const {Readable} = require('node:stream');
const files = require('./files.cjs');
const {spacePaths} = require('./spaces.cjs');
const {createStreamCutsService} = require('./streamcuts.cjs');

const ACCEPTS = {
  forma: file => ['image', 'video'].includes(file.kind) || /\.(forma|json)$/i.test(file.name) && !/\.folio\.json$/i.test(file.name),
  folio: file => /\.(png|jpe?g|webp)$/i.test(file.name) || /\.folio\.json$/i.test(file.name),
  streamcuts: file => ['image', 'video', 'audio'].includes(file.kind)
};
const MAX_PANES = 2;

function createWorkspace({window, router, catalog, modules, spaces, emit, logDir}) {
  let space = null, paths = null;
  const instances = new Map(); // app id -> {entry, host, view, status, message, ready}
  let visible = [], focused = null, split = false, overlay = false;
  const lastFocus = new Map();
  const rects = new Map();
  const services = new Map();
  const preparedSessions = new WeakSet();
  const deliveries = new Map();
  let watchers = [];

  const toast = text => emit('toast', text);
  const entryFor = id => catalog.find(entry => entry.id === id);
  const available = entry => entry && !entry.problem;

  function state() {
    return {
      screen: space ? 'space' : 'home',
      space: space && {id: space.id, name: space.name, color: space.color, dir: space.dir},
      apps: catalog.map(entry => {
        const instance = instances.get(entry.id);
        return {id: entry.id, name: entry.name, description: entry.description, accent: entry.accent, initial: entry.initial,
          problem: entry.problem, running: !!instance, status: instance?.status || null, message: instance?.message || null};
      }),
      visible, focused, split
    };
  }
  const changed = () => { apply(); emit('state', state()); };

  // Places each running app's view over its pane, or hides it.
  function apply() {
    for (const [id, instance] of instances) {
      if (!instance.view) continue;
      const rect = rects.get(id);
      const show = !overlay && instance.status === 'ready' && visible.includes(id) && rect && rect.width > 0 && rect.height > 0;
      if (show) instance.view.setBounds(rect);
      instance.view.setVisible(!!show);
    }
  }

  function layout(panes) {
    rects.clear();
    for (const pane of Array.isArray(panes) ? panes : []) {
      if (!instances.has(pane.app)) continue;
      rects.set(pane.app, {x: Math.round(pane.x), y: Math.round(pane.y), width: Math.max(0, Math.round(pane.width)), height: Math.max(0, Math.round(pane.height))});
    }
    apply();
  }
  function setOverlay(value) { overlay = !!value; apply(); }

  // Downloads from any app (Forma's exports, StreamCuts clip exports) land in the space's Files.
  function prepareSession(ses) {
    if (preparedSessions.has(ses)) return;
    preparedSessions.add(ses);
    ses.on('will-download', (_, item) => {
      if (!paths) return;
      fs.mkdirSync(paths.files, {recursive: true});
      const name = files.uniqueNameSync(paths.files, item.getFilename());
      item.setSavePath(path.join(paths.files, name));
      item.once('done', (__, result) => {
        if (result === 'completed') toast(`Saved ${name} to Files`);
        else if (result !== 'cancelled') toast(`${name} couldn't be saved`);
      });
    });
  }
  // canopy-file://files/<name> serves the space's Files to web apps (with byte ranges, so video can seek).
  function serveFiles(ses, filesDir) {
    if (ses.protocol.isProtocolHandled('canopy-file')) ses.protocol.unhandle('canopy-file');
    ses.protocol.handle('canopy-file', async request => {
      const url = new URL(request.url);
      let file;
      try { file = files.resolveInside(filesDir, decodeURIComponent(url.pathname.slice(1))); } catch { return new Response('Not found', {status: 404}); }
      if (url.hostname !== 'files') return new Response('Not found', {status: 404});
      let size;
      try { size = (await fsp.stat(file)).size; } catch { return new Response('Not found', {status: 404}); }
      const headers = {'Content-Type': files.typeOf(file), 'Accept-Ranges': 'bytes', 'Access-Control-Allow-Origin': '*'};
      const range = /^bytes=(\d*)-(\d*)$/.exec(request.headers.get('range') || '');
      if (!range || (!range[1] && !range[2])) return new Response(Readable.toWeb(fs.createReadStream(file)), {headers: {...headers, 'Content-Length': String(size)}});
      const start = range[1] ? Number(range[1]) : Math.max(0, size - Number(range[2]));
      const end = range[1] && range[2] ? Math.min(Number(range[2]), size - 1) : size - 1;
      if (start >= size || start > end) return new Response(null, {status: 416, headers: {...headers, 'Content-Range': `bytes */${size}`}});
      return new Response(Readable.toWeb(fs.createReadStream(file, {start, end})), {status: 206, headers: {...headers, 'Content-Range': `bytes ${start}-${end}/${size}`, 'Content-Length': String(end - start + 1)}});
    });
  }

  async function saveFile({name, data}) {
    if (!paths) throw new Error('No space is open.');
    const saved = await files.writeInto(paths.files, name, data);
    toast(`Saved ${saved} to Files`);
    return {name: saved, path: path.join(paths.files, saved)};
  }

  // App shortcuts work wherever the keyboard focus is, including inside an app.
  function watchShortcuts(contents) {
    contents.on('before-input-event', (event, input) => {
      if (input.type !== 'keyDown' || !input.control || input.alt || input.meta) return;
      if (input.key === 'Tab') { event.preventDefault(); cycle(input.shift ? -1 : 1); return; }
      const index = /^[1-9]$/.test(input.key) && !input.shift ? Number(input.key) - 1 : -1;
      if (index >= 0 && catalog[index]) { event.preventDefault(); openApp(catalog[index].id).catch(() => {}); }
    });
  }
  function makeView({preload, ses, args, id}) {
    const view = new WebContentsView({webPreferences: {preload, session: ses, contextIsolation: true, nodeIntegration: false, sandbox: true, backgroundThrottling: false, additionalArguments: args}});
    view.setBackgroundColor('#0C0A05');
    view.setVisible(false);
    window.contentView.addChildView(view);
    watchShortcuts(view.webContents);
    view.webContents.on('focus', () => {
      if (focused !== id) { focused = id; lastFocus.set(id, Date.now()); emit('state', state()); }
    });
    view.webContents.on('render-process-gone', (_, details) => {
      const instance = instances.get(id);
      if (instance && details.reason !== 'clean-exit') { instance.status = 'failed'; instance.message = `${instance.entry.name} stopped unexpectedly (${details.reason}). Close it and open it again.`; changed(); }
    });
    return view;
  }

  async function startElectronApp(instance) {
    const {entry} = instance;
    const module = modules.get(entry.id);
    const problem = module?.check?.();
    if (problem) throw new Error(problem);
    const ses = session.fromPartition(`persist:canopy-${space.id}-${entry.id}`);
    prepareSession(ses);
    let view = null;
    const owner = `${space.id}:${entry.id}:${crypto.randomUUID()}`;
    instance.host = module.create({
      ipc: router.scope(owner), session: ses, window: () => window, webContents: () => view?.webContents,
      dataDir: paths.appData(entry.id), documentsDir: app.getPath('documents'), exportDirectory: paths.files,
      // Forma's MCP server looks for the editor where standalone Forma announces itself.
      discoveryFile: path.join(app.getPath('appData'), 'forma-animation-studio', 'mcp-bridge.json'),
      canopy: {space: {id: space.id, name: space.name, dir: space.dir}, projectsDir: paths.folio, filesDir: paths.files, saveFile}
    });
    view = makeView({preload: instance.host.preload, ses, args: instance.host.additionalArguments, id: entry.id});
    instance.view = view;
    router.claim(view.webContents, owner);
    view.webContents.setWindowOpenHandler(() => ({action: 'deny'}));
    view.webContents.on('will-navigate', event => event.preventDefault());
    await view.webContents.loadFile(instance.host.entry);
  }

  async function startWebApp(instance) {
    const {entry} = instance;
    if (!services.has(entry.id)) services.set(entry.id, createStreamCutsService({root: entry.dir, logFile: path.join(logDir, 'streamcuts.log')}));
    const url = await services.get(entry.id).start(message => { instance.message = message; emit('state', state()); });
    if (!instances.has(entry.id) || instances.get(entry.id) !== instance) return;
    const ses = session.fromPartition(`persist:canopy-${space.id}-${entry.id}`);
    prepareSession(ses);
    serveFiles(ses, paths.files);
    const origin = new URL(url).origin;
    const view = makeView({preload: path.join(__dirname, '..', 'preloads', 'web-app.cjs'), ses, id: entry.id,
      args: ['--canopy=' + JSON.stringify({app: entry.id, space: space.name})]});
    instance.view = view;
    const external = target => { try { const link = new URL(target); if (['http:', 'https:'].includes(link.protocol)) shell.openExternal(link.href); } catch {} };
    view.webContents.setWindowOpenHandler(({url: target}) => {
      if (target.startsWith(origin)) view.webContents.loadURL(target);
      else external(target);
      return {action: 'deny'};
    });
    view.webContents.on('will-navigate', (event, target) => { if (!target.startsWith(origin)) { event.preventDefault(); external(target); } });
    instance.host = {
      receive: list => deliverToWeb(view, list),
      listDocuments: async () => [], openDocument: async () => {}, checkpoint: async () => {}, dispose: async () => {}
    };
    await view.webContents.loadURL(url);
  }

  // Web apps get files through the Canopy preload, which turns them into a paste (StreamCuts adds pasted media at the
  // playhead). The preload reports whether the page took it.
  function deliverToWeb(view, list) {
    const id = crypto.randomUUID();
    const items = list.map(file => ({name: file.name, type: files.typeOf(file.name), url: 'canopy-file://files/' + encodeURIComponent(file.name)}));
    return new Promise(resolve => {
      const timer = setTimeout(() => { deliveries.delete(id); resolve({accepted: 0, skipped: list.map(file => file.name)}); }, 120000);
      deliveries.set(id, result => { clearTimeout(timer); resolve(result.ok ? {accepted: list.length, skipped: []} : {accepted: 0, skipped: list.map(file => file.name), reason: result.reason}); });
      view.webContents.send('canopy:deliver', {id, items});
    });
  }
  function delivered(_, {id, ok, reason} = {}) {
    const finish = deliveries.get(id);
    if (finish) { deliveries.delete(id); finish({ok: !!ok, reason}); }
  }

  // Starts an app in this space (once) and resolves when it can take files.
  function ensureApp(id) {
    const entry = entryFor(id);
    if (!space) return Promise.reject(new Error('Open a space first.'));
    if (!available(entry)) return Promise.reject(new Error(entry?.problem || 'Unknown app.'));
    if (instances.has(id)) return instances.get(id).ready;
    const instance = {entry, host: null, view: null, status: 'starting', message: `Opening ${entry.name}…`};
    instances.set(id, instance);
    instance.ready = (entry.kind === 'web' ? startWebApp(instance) : startElectronApp(instance)).then(() => {
      if (instances.get(id) !== instance) throw new Error(`${entry.name} was closed.`);
      instance.status = 'ready'; instance.message = null; changed();
      return instance;
    }, error => {
      instance.status = 'failed'; instance.message = error.message; changed();
      throw error;
    });
    instance.ready.catch(() => {});
    changed();
    return instance.ready;
  }

  function show(id) {
    if (!visible.includes(id)) {
      if (!split) visible = [id];
      else if (visible.length < MAX_PANES) visible = [...visible, id];
      else {
        // Opening a third app replaces the pane you used least recently.
        const oldest = visible.reduce((a, b) => ((lastFocus.get(a) || 0) <= (lastFocus.get(b) || 0) ? a : b));
        visible = visible.map(item => (item === oldest ? id : item));
      }
    }
    focused = id;
    lastFocus.set(id, Date.now());
    instances.get(id)?.view?.webContents.focus();
    saveLayout();
    changed();
  }

  async function openApp(id) {
    const starting = ensureApp(id);
    show(id);
    const instance = await starting;
    instance.view?.webContents.focus();
    return instance;
  }

  function cycle(direction) {
    if (!space) return;
    const order = catalog.filter(available).map(entry => entry.id);
    if (!order.length) return;
    const current = order.indexOf(focused);
    const next = order[(current + direction + order.length) % order.length];
    openApp(next).catch(() => {});
  }

  function setSplit(value) {
    split = !!value;
    if (!split) visible = visible.filter(id => id === focused).slice(0, 1);
    else if (visible.length < MAX_PANES) {
      // Splitting shows another app beside the current one: one already running, else the next in the dock.
      const others = catalog.filter(entry => available(entry) && !visible.includes(entry.id));
      const pick = others.find(entry => instances.has(entry.id)) || others[0];
      if (pick) { ensureApp(pick.id).catch(() => {}); visible = [...visible, pick.id]; }
    }
    saveLayout();
    changed();
  }

  async function closeApp(id) {
    const instance = instances.get(id);
    if (!instance) return;
    instances.delete(id);
    visible = visible.filter(item => item !== id);
    if (focused === id) focused = visible[0] || null;
    if (instance.status === 'ready') {
      try { await instance.host.checkpoint(); } catch (error) { toast(`${instance.entry.name} couldn't save: ${error.message}`); }
    }
    await stopInstance(instance);
    saveLayout();
    changed();
  }
  async function stopInstance(instance) {
    try { await instance.host?.dispose(); } catch {}
    if (instance.view) {
      window.contentView.removeChildView(instance.view);
      instance.view.webContents.close();
    }
  }

  // Remembers which apps were showing, so a space reopens the way you left it.
  function saveLayout() {
    if (!paths) return;
    const file = path.join(paths.appData('canopy'), 'workspace.json');
    fsp.mkdir(path.dirname(file), {recursive: true}).then(() => fsp.writeFile(file, JSON.stringify({visible, split, focused}))).catch(() => {});
  }
  async function readLayout() {
    try { return JSON.parse(await fsp.readFile(path.join(paths.appData('canopy'), 'workspace.json'), 'utf8')); } catch { return null; }
  }

  function watch() {
    let filesTimer, documentsTimer;
    const filesChanged = () => { clearTimeout(filesTimer); filesTimer = setTimeout(() => emit('files'), 250); };
    const documentsChanged = () => { clearTimeout(documentsTimer); documentsTimer = setTimeout(() => emit('documents'), 500); };
    const add = (dir, listener, options = {}) => {
      try { fs.mkdirSync(dir, {recursive: true}); watchers.push(fs.watch(dir, options, listener)); } catch {}
    };
    add(paths.files, filesChanged);
    add(paths.folio, documentsChanged);
    add(path.join(space.dir, '.canopy'), documentsChanged, {recursive: true});
  }

  // Lets every running app save, then stops them. Returns the apps that couldn't save.
  async function stopAll() {
    const failures = [];
    const all = [...instances.values()];
    instances.clear();
    await Promise.all(all.map(async instance => {
      try {
        let timer;
        await Promise.race([instance.host?.checkpoint(), new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('it did not respond')), 5000); })]).finally(() => clearTimeout(timer));
      } catch (error) { if (instance.status === 'ready') failures.push(`${instance.entry.name}: ${error.message}`); }
      await stopInstance(instance);
    }));
    return failures;
  }
  // Saves each app's work without stopping it (used before Canopy closes, so it can ask you first if one fails).
  async function checkpointAll() {
    const failures = [];
    await Promise.all([...instances.values()].map(async instance => {
      if (instance.status !== 'ready') return;
      try {
        let timer;
        await Promise.race([instance.host.checkpoint(), new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('it did not respond')), 5000); })]).finally(() => clearTimeout(timer));
      } catch (error) { failures.push(`${instance.entry.name}: ${error.message}`); }
    }));
    return failures;
  }

  async function closeSpace() {
    if (!space) return [];
    for (const watcher of watchers) watcher.close();
    watchers = [];
    const failures = await stopAll();
    space = null; paths = null; visible = []; focused = null; split = false; rects.clear();
    changed();
    return failures;
  }

  async function openSpace(id) {
    if (space?.id === id) return state();
    const failures = await closeSpace();
    if (failures.length) toast(`Some work couldn't be saved: ${failures.join('; ')}`);
    space = await spaces.touch(id);
    paths = spacePaths(space.dir);
    watch();
    const saved = await readLayout();
    split = !!saved?.split;
    changed();
    emit('files'); emit('documents');
    for (const appId of (saved?.visible || []).filter(appId => available(entryFor(appId)))) {
      ensureApp(appId).catch(() => {});
      if (!visible.includes(appId)) visible = [...visible, appId].slice(-(split ? MAX_PANES : 1));
    }
    focused = saved?.focused && visible.includes(saved.focused) ? saved.focused : visible[0] || null;
    changed();
    return state();
  }

  async function shutdown() {
    const failures = await closeSpace();
    await Promise.all([...services.values()].map(service => service.stop()));
    return failures;
  }

  // ---- Files ----
  async function listSpaceFiles() { return paths ? files.listFiles(paths.files) : []; }
  async function addPaths(sources) {
    if (!paths) throw new Error('Open a space first.');
    const added = [];
    for (const source of sources) {
      // A file dragged out of the sidebar and dropped back on it is already here.
      if (path.resolve(path.dirname(source)) === path.resolve(paths.files)) continue;
      try { added.push(await files.copyInto(paths.files, source)); }
      catch (error) { toast(`${path.basename(source)} couldn't be added: ${error.message}`); }
    }
    if (added.length) toast(added.length === 1 ? `Added ${added[0]} to Files` : `Added ${added.length} files to Files`);
    return added;
  }
  function filePath(name) {
    if (!paths) throw new Error('Open a space first.');
    return files.resolveInside(paths.files, name);
  }
  async function describe(name) {
    const full = filePath(name);
    const stat = await fsp.stat(full);
    return {path: full, name, size: stat.size, type: files.typeOf(name), kind: files.kindOf(name)};
  }

  // Hands files from the space to an app, opening it if needed.
  async function sendFiles(names, appId) {
    const entry = entryFor(appId);
    const list = await Promise.all(names.map(describe));
    const instance = await openApp(appId);
    const result = await instance.host.receive(list);
    if (result.accepted && !result.skipped.length) toast(list.length === 1 ? `Sent ${list[0].name} to ${entry.name}` : `Sent ${list.length} files to ${entry.name}`);
    else if (result.reason) toast(result.reason);
    else if (result.skipped.length) toast(`${entry.name} can't open ${result.skipped.join(', ')}`);
    return result;
  }

  let dragIcon = null;
  function startDrag(contents, name) {
    // A small leaf under the pointer while a file is dragged into an app.
    dragIcon ??= (() => {
      const size = 32, bitmap = Buffer.alloc(size * size * 4);
      for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
        const corner = (x < 16) === (y < 16) ? 15 : 4, cx = x < 16 ? corner : size - 1 - corner, cy = y < 16 ? corner : size - 1 - corner;
        const inside = (x >= corner && x <= size - 1 - corner) || (y >= corner && y <= size - 1 - corner) || Math.hypot(x - cx, y - cy) <= corner;
        if (inside) bitmap.set([0x3f, 0xc6, 0x8c, 0xff], (y * size + x) * 4);
      }
      return nativeImage.createFromBitmap(bitmap, {width: size, height: size});
    })();
    contents.startDrag({file: filePath(name), icon: dragIcon});
  }

  function fileMenu(name) {
    const file = {name, kind: files.kindOf(name)};
    const template = catalog.map(entry => ({
      label: `Send to ${entry.name}`,
      enabled: available(entry) && ACCEPTS[entry.id]?.(file),
      click: () => sendFiles([name], entry.id).catch(error => toast(error.message))
    }));
    template.push({type: 'separator'},
      {label: 'Show in folder', click: () => shell.showItemInFolder(filePath(name))},
      {label: 'Move to Recycle Bin', click: () => shell.trashItem(filePath(name)).then(() => emit('files')).catch(error => toast(error.message))});
    Menu.buildFromTemplate(template).popup({window});
  }
  // Double-clicking a file sends it to the app on screen when that app can take it, else to the first app that can.
  function openFile(name) {
    const file = {name, kind: files.kindOf(name)};
    const target = [focused, ...catalog.map(entry => entry.id)].find(id => id && available(entryFor(id)) && ACCEPTS[id]?.(file));
    if (!target) { toast(`None of your apps can open ${name}`); return Promise.resolve(); }
    return sendFiles([name], target).catch(error => toast(error.message));
  }

  // ---- Projects (documents) ----
  async function listDocuments() {
    if (!paths) return [];
    const result = [];
    for (const entry of catalog) {
      const module = modules.get(entry.id);
      if (!available(entry) || !module?.documents) continue;
      try {
        for (const doc of await module.documents({dataDir: paths.appData(entry.id), canopy: {projectsDir: paths.folio, filesDir: paths.files}})) result.push({...doc, app: entry.id});
      } catch {}
    }
    return result.sort((a, b) => (b.savedAt || 0) - (a.savedAt || 0));
  }
  async function openDocument(appId, id) {
    const instance = await openApp(appId);
    await instance.host.openDocument(id);
  }

  return {
    state, layout, setOverlay, openSpace, closeSpace, shutdown, checkpointAll, openApp, closeApp, setSplit, cycle,
    focusApp: id => instances.has(id) ? show(id) : openApp(id).catch(() => {}),
    listFiles: listSpaceFiles, addPaths, sendFiles, startDrag, fileMenu, openFile, revealFile: name => shell.showItemInFolder(filePath(name)),
    revealSpace: () => space && shell.openPath(space.dir), listDocuments, openDocument, delivered, watchShortcuts,
    currentSpace: () => space, saveFile
  };
}

module.exports = {createWorkspace, ACCEPTS};

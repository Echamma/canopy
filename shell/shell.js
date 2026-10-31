// Canopy's shell: the "Where to?" screen, and inside a space the tabs, sidebar and pane chrome. The apps themselves are
// native views the main process lays over each pane body; this page reports where those bodies are.
const api = window.canopy;
const $ = id => document.getElementById(id);
const ACCENTS = {leaf: '--app-leaf', sky: '--app-sky', clay: '--app-clay', berry: '--app-berry', moss: '--app-moss'};
const COLORS = Object.keys(ACCENTS);
const ICONS = {
  chevron: '<path d="m6 9 6 6 6-6"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  folder: '<path d="M3 6h6l2 2h10v11H3z"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>',
  split: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M12 4v16"/>',
  close: '<path d="M6 6l12 12M18 6 6 18"/>',
  image: '<rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="9" cy="10" r="2"/><path d="m21 17-5-5-9 8"/>',
  video: '<rect x="3" y="5" width="13" height="14" rx="2"/><path d="m16 10 5-3v10l-5-3"/>',
  audio: '<path d="M9 18V6l10-2v12"/><circle cx="6" cy="18" r="3"/><circle cx="16" cy="16" r="3"/>',
  archive: '<path d="M4 7h16v13H4zM3 4h18v3H3zM10 11h4"/>',
  project: '<path d="M6 3h8l4 4v14H6zM14 3v4h4M9 12h6M9 16h6"/>',
  file: '<path d="M6 3h8l4 4v14H6zM14 3v4h4"/>',
  open: '<path d="M3 6h6l2 2h10v11H3zM12 11v5M9.5 13.5h5"/>'
};
const icon = (name, size = 16) => `<svg class="icon" width="${size}" height="${size}" viewBox="0 0 24 24" aria-hidden="true">${ICONS[name]}</svg>`;

let state = null, files = [], documents = [], query = '', toastTimer = null, freshFiles = new Set();

function leaf(color, size, label = '') {
  const element = document.createElement('span');
  element.className = `leaf leaf-${size}` + (color === 'leaf' ? ' ink-dark' : '');
  element.style.setProperty('--accent', `var(${ACCENTS[color] || ACCENTS.leaf})`);
  element.setAttribute('aria-hidden', 'true');
  if (label) element.textContent = label;
  return element;
}
function button(className, label, onClick) {
  const element = document.createElement('button');
  element.type = 'button';
  element.className = className;
  if (label) element.setAttribute('aria-label', label);
  if (onClick) element.addEventListener('click', onClick);
  return element;
}
const size = bytes => bytes < 1024 ? `${bytes} B` : bytes < 1048576 ? `${Math.round(bytes / 1024)} KB` : bytes < 1073741824 ? `${(bytes / 1048576).toFixed(1)} MB` : `${(bytes / 1073741824).toFixed(2)} GB`;
const appById = id => state?.apps.find(item => item.id === id);
const matches = text => !query || text.toLowerCase().includes(query);

// A short message in the top bar. A file an app just saved comes with buttons to send it on to the other apps.
function toast(value) {
  const {text, file, targets = []} = typeof value === 'string' ? {text: value} : value;
  const element = $('toast');
  const label = document.createElement('span');
  label.className = 'toast-text';
  label.textContent = text;
  label.title = text;
  element.replaceChildren(label);
  const hide = () => element.classList.remove('show');
  for (const id of targets) {
    const item = appById(id);
    if (!item) continue;
    const send = button('toast-action', null, () => { hide(); api.sendFile(file, id); });
    send.append(leaf(item.accent, 14), document.createTextNode(`Send to ${item.name}`));
    element.append(send);
  }
  element.classList.add('show');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(hide, targets.length ? 10000 : 4000);
}

// ---- Where to? ----
async function renderHome() {
  const list = await api.listSpaces();
  const tiles = $('space-tiles');
  tiles.replaceChildren();
  for (const space of list) {
    const tile = button('space-tile' + (space.available ? '' : ' missing'), null, async () => {
      try { await api.openSpace(space.id); } catch (error) { $('home-note').textContent = error.message.replace(/^Error invoking remote method '[^']+': (Error: )?/, ''); }
    });
    tile.title = space.available ? space.dir : `Folder not found: ${space.dir}`;
    const name = document.createElement('span');
    name.className = 'name';
    name.textContent = space.name;
    tile.append(leaf(space.color, 68), name);
    tiles.append(tile);
  }
  const create = button('space-tile', null, openNewSpace);
  const box = document.createElement('span');
  box.className = 'new-tile';
  box.innerHTML = icon('plus', 20);
  const label = document.createElement('span');
  label.className = 'name';
  label.textContent = 'New space';
  create.append(box, label);
  tiles.append(create);
  $('home-note').textContent = list.length ? '' : 'A space keeps your Forma, Folio and StreamCuts work together, with the files they share.';
}

// ---- A space ----
function renderWorkspace() {
  const {space, apps, visible, focused, split} = state;
  for (const id of ['space-leaf', 'launcher-leaf']) $(id).style.setProperty('--accent', `var(${ACCENTS[space.color] || ACCENTS.leaf})`);
  $('space-name').textContent = space.name;
  $('launcher-space').textContent = space.name;
  document.title = `${space.name} — Canopy`;
  $('split-button').setAttribute('aria-pressed', String(split));

  const tabs = $('app-tabs');
  tabs.replaceChildren();
  for (const item of apps) {
    const tab = button('tab', null, () => api.openApp(item.id));
    tab.disabled = !!item.problem;
    tab.title = item.problem || `${item.name}: ${item.description}`;
    if (visible.includes(item.id) && item.id === focused) tab.setAttribute('aria-current', 'true');
    const name = document.createElement('span');
    name.textContent = item.name;
    tab.append(leaf(item.accent, 14), name);
    if (item.running) {
      const dot = document.createElement('span');
      dot.className = 'dot' + (item.status === 'ready' ? '' : ' ' + item.status);
      dot.title = item.status === 'ready' ? 'Running' : item.status === 'starting' ? 'Starting' : 'Needs attention';
      tab.append(dot);
    }
    tabs.append(tab);
  }

  const dock = $('dock');
  dock.replaceChildren();
  for (const item of apps) {
    const open = button('ib', `Open ${item.name}`, () => api.openApp(item.id));
    open.disabled = !!item.problem;
    open.title = item.problem || `Open ${item.name}`;
    open.append(leaf(item.accent, 18));
    dock.append(open);
  }

  const launcherTiles = $('launcher-tiles');
  launcherTiles.replaceChildren();
  for (const item of apps) {
    const tile = button('tile', null, () => api.openApp(item.id));
    tile.disabled = !!item.problem;
    tile.title = item.problem || item.description;
    const box = document.createElement('span');
    box.className = 'tile-box';
    box.append(leaf(item.accent, 30, item.initial));
    const name = document.createElement('span');
    name.textContent = item.name;
    const description = document.createElement('small');
    description.textContent = item.problem ? 'Not set up' : item.description;
    tile.append(box, name, description);
    launcherTiles.append(tile);
  }

  $('launcher').hidden = visible.length > 0;
  $('panes').hidden = visible.length === 0;
  renderPanes();
}

function renderPanes() {
  const {visible, focused, split} = state;
  const panes = $('panes');
  const existing = new Map([...panes.children].map(element => [element.dataset.app, element]));
  for (const [id, element] of existing) if (!visible.includes(id)) element.remove();
  visible.forEach((id, index) => {
    const item = appById(id);
    if (!item) return;
    let pane = existing.get(id);
    if (!pane) {
      pane = document.createElement('section');
      pane.className = 'pane';
      pane.dataset.app = id;
      const head = document.createElement('div');
      head.className = 'pane-head';
      const pill = document.createElement('span');
      pill.className = 'pill';
      const name = document.createElement('span');
      name.textContent = item.name;
      pill.append(leaf(item.accent, 14), name);
      const status = document.createElement('span');
      status.className = 'pane-status';
      const spacer = document.createElement('div');
      spacer.className = 'spacer';
      const splitButton = button('ib', null, () => api.setSplit(!state.split));
      splitButton.dataset.role = 'split';
      splitButton.innerHTML = icon('split', 14);
      const close = button('ib', `Close ${item.name}`, () => api.closeApp(id));
      close.title = `Close ${item.name}`;
      close.innerHTML = icon('close', 14);
      head.append(pill, status, spacer, splitButton, close);
      const body = document.createElement('div');
      body.className = 'pane-body';
      body.dataset.app = id;
      pane.append(head, body);
      pane.addEventListener('pointerdown', () => { if (state.focused !== id) api.focusApp(id); });
    }
    panes.insertBefore(pane, panes.children[index] || null);
    pane.classList.toggle('focused', id === focused && visible.length > 1);
    const splitButton = pane.querySelector('[data-role="split"]');
    splitButton.setAttribute('aria-label', split ? 'Show one app' : 'Show two apps side by side');
    splitButton.title = splitButton.getAttribute('aria-label');
    pane.querySelector('.pane-status').textContent = item.status === 'starting' ? 'Starting…' : '';
    const body = pane.querySelector('.pane-body');
    const signature = `${item.status}|${item.message}`;
    if (body.dataset.signature !== signature) {
      body.dataset.signature = signature;
      body.replaceChildren();
      if (item.status !== 'ready') {
        const message = document.createElement('div');
        message.className = 'pane-message' + (item.status === 'failed' ? ' failed' : '');
        if (item.status === 'starting') { const spinner = document.createElement('span'); spinner.className = 'spinner'; message.append(spinner); }
        const text = document.createElement('p');
        text.textContent = item.message || (item.status === 'failed' ? `${item.name} couldn't open.` : `Opening ${item.name}…`);
        message.append(text);
        if (item.status === 'failed') {
          const retry = button('small-btn', null, async () => { await api.closeApp(id); api.openApp(id); });
          retry.textContent = 'Try again';
          message.append(retry);
        }
        body.append(message);
      }
    }
  });
  scheduleLayout();
}

let layoutFrame = 0;
function scheduleLayout() {
  cancelAnimationFrame(layoutFrame);
  layoutFrame = requestAnimationFrame(() => {
    const bodies = state?.screen === 'space' ? [...document.querySelectorAll('.pane-body')] : [];
    api.layout(bodies.map(body => {
      const rect = body.getBoundingClientRect();
      return {app: body.dataset.app, x: rect.left, y: rect.top, width: rect.width, height: rect.height};
    }));
  });
}
new ResizeObserver(scheduleLayout).observe(document.body);

// ---- Sidebar ----
function renderDocuments() {
  const list = $('documents');
  list.replaceChildren();
  const shown = documents.filter(doc => matches(doc.name));
  for (const doc of shown) {
    const item = appById(doc.app);
    const row = button('row', null, () => api.openDocument(doc.app, doc.id));
    row.title = `Open in ${item?.name || doc.app}`;
    const text = document.createElement('span');
    text.className = 'text';
    const name = document.createElement('span');
    name.className = 'name';
    name.textContent = doc.name;
    name.dir = 'auto';
    const meta = document.createElement('span');
    meta.className = 'meta';
    meta.textContent = [item?.name, doc.detail].filter(Boolean).join(' · ');
    text.append(name, meta);
    row.append(leaf(item?.accent, 16), text);
    const li = document.createElement('li');
    li.append(row);
    list.append(li);
  }
  if (!shown.length) {
    const empty = document.createElement('li');
    empty.className = 'empty';
    empty.textContent = query ? 'No projects match.' : 'No projects yet. Open Forma or Folio to start one.';
    list.append(empty);
  }
}

function renderFiles() {
  const list = $('files');
  list.replaceChildren();
  const shown = files.filter(file => matches(file.name));
  for (const file of shown) {
    const row = button('row' + (freshFiles.has(file.name) ? ' fresh' : ''), null);
    row.draggable = true;
    row.title = `${file.name}\nDouble-click to open it in an app, drag it into one, or right-click for more.`;
    const kind = document.createElement('span');
    kind.className = 'kind';
    kind.innerHTML = icon(ICONS[file.kind] ? file.kind : 'file');
    const text = document.createElement('span');
    text.className = 'text';
    const name = document.createElement('span');
    name.className = 'name';
    name.textContent = file.name;
    const meta = document.createElement('span');
    meta.className = 'meta';
    meta.textContent = size(file.size);
    text.append(name, meta);
    row.append(kind, text);
    row.addEventListener('dblclick', () => api.openFile(file.name));
    row.addEventListener('keydown', event => {
      if (event.key === 'Enter') api.openFile(file.name);
      if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) { event.preventDefault(); api.fileMenu(file.name); }
    });
    row.addEventListener('contextmenu', event => { event.preventDefault(); api.fileMenu(file.name); });
    row.addEventListener('dragstart', event => { event.preventDefault(); api.startDrag(file.name); });
    const li = document.createElement('li');
    li.append(row);
    list.append(li);
  }
  if (!shown.length) {
    const empty = document.createElement('li');
    empty.className = 'empty';
    const text = document.createElement('span');
    text.textContent = query ? 'No files match.' : 'No files yet. Exports from your apps land here.';
    empty.append(text);
    if (!query) {
      const add = button('small-btn', null, () => api.addFiles());
      add.innerHTML = icon('plus', 14) + '<span>Add files</span>';
      empty.append(add);
    }
    list.append(empty);
  }
}

async function refreshFiles() {
  if (state?.screen !== 'space') return;
  const before = new Set(files.map(file => file.name));
  files = await api.listFiles();
  if (before.size) for (const file of files) if (!before.has(file.name)) {
    freshFiles.add(file.name);
    setTimeout(() => { freshFiles.delete(file.name); renderFiles(); }, 3000);
  }
  renderFiles();
}
async function refreshDocuments() {
  if (state?.screen !== 'space') return;
  documents = await api.listDocuments();
  renderDocuments();
}

// ---- New space ----
let spaceParent = null;
async function openNewSpace() {
  $('space-name-input').value = '';
  $('new-space-error').textContent = '';
  spaceParent = null;
  $('space-location').textContent = await api.defaultSpaceFolder();
  $('space-location').title = $('space-location').textContent;
  api.setOverlay(true);
  $('new-space').showModal();
  $('space-name-input').focus();
}
function setupNewSpace() {
  const chips = $('color-chips');
  COLORS.forEach((color, index) => {
    const chip = document.createElement('label');
    chip.className = 'chip';
    const input = document.createElement('input');
    input.type = 'radio';
    input.name = 'space-color';
    input.value = color;
    input.checked = index === 0;
    input.setAttribute('aria-label', color[0].toUpperCase() + color.slice(1));
    chip.append(input, leaf(color, 30));
    chips.append(chip);
  });
  $('new-space').addEventListener('close', () => api.setOverlay(false));
  $('new-space-cancel').addEventListener('click', () => $('new-space').close());
  $('space-location-change').addEventListener('click', async () => {
    const chosen = await api.chooseSpaceFolder();
    if (chosen) { spaceParent = chosen; $('space-location').textContent = chosen; $('space-location').title = chosen; }
  });
  $('new-space-form').addEventListener('submit', async event => {
    event.preventDefault();
    const name = $('space-name-input').value.trim();
    if (!name) { $('new-space-error').textContent = 'Give your space a name.'; return; }
    const color = document.querySelector('input[name="space-color"]:checked')?.value;
    $('new-space-create').disabled = true;
    try {
      await api.createSpace({name, color, parent: spaceParent});
      $('new-space').close();
    } catch (error) {
      $('new-space-error').textContent = error.message.replace(/^Error invoking remote method '[^']+': (Error: )?/, '');
    } finally { $('new-space-create').disabled = false; }
  });
}

// ---- Wiring ----
async function render(next) {
  const previous = state;
  state = next;
  document.body.dataset.mood = state.screen === 'home' ? 'tide' : 'ember';
  $('home').hidden = state.screen !== 'home';
  $('workspace').hidden = state.screen !== 'space';
  if (state.screen === 'home') { document.title = 'Canopy'; await renderHome(); scheduleLayout(); return; }
  renderWorkspace();
  if (previous?.space?.id !== state.space.id) {
    files = []; documents = []; query = ''; $('search').value = '';
    refreshFiles(); refreshDocuments();
  }
}

function setup() {
  $('home-open-folder').innerHTML = icon('open', 18);
  $('home-open-folder').addEventListener('click', () => api.addSpaceFolder().catch(error => { $('home-note').textContent = error.message.replace(/^Error invoking remote method '[^']+': (Error: )?/, ''); }));
  document.querySelector('#space-pill .chev').innerHTML = icon('chevron', 14);
  document.querySelector('.search-icon').innerHTML = icon('search', 15);
  $('add-files').innerHTML = icon('plus');
  $('reveal-space').innerHTML = icon('folder');
  $('split-button').innerHTML = icon('split');
  $('home-button').addEventListener('click', () => api.goHome());
  $('space-pill').addEventListener('click', () => api.spaceMenu());
  $('split-button').addEventListener('click', () => api.setSplit(!state.split));
  $('add-files').addEventListener('click', () => api.addFiles());
  $('reveal-space').addEventListener('click', () => api.revealSpace());
  $('search').addEventListener('input', () => { query = $('search').value.trim().toLowerCase(); renderFiles(); renderDocuments(); });

  // Files dropped on the sidebar from Explorer are copied into the space's Files.
  const sidebar = $('sidebar');
  let depth = 0;
  sidebar.addEventListener('dragenter', event => { if (event.dataTransfer.types.includes('Files')) { depth++; sidebar.classList.add('dropping'); } });
  sidebar.addEventListener('dragleave', () => { if (--depth <= 0) { depth = 0; sidebar.classList.remove('dropping'); } });
  sidebar.addEventListener('dragover', event => { if (event.dataTransfer.types.includes('Files')) { event.preventDefault(); event.dataTransfer.dropEffect = 'copy'; } });
  sidebar.addEventListener('drop', event => {
    event.preventDefault();
    depth = 0;
    sidebar.classList.remove('dropping');
    if (event.dataTransfer.files.length) api.addDroppedFiles(event.dataTransfer.files);
  });
  // Anything else dropped on the shell is ignored rather than opened as a page.
  document.addEventListener('dragover', event => event.preventDefault());
  document.addEventListener('drop', event => event.preventDefault());

  const tick = () => { $('clock').textContent = new Date().toLocaleTimeString([], {hour: 'numeric', minute: '2-digit'}); };
  tick();
  setInterval(tick, 15000);

  setupNewSpace();
  api.onState(render);
  api.onFiles(refreshFiles);
  api.onDocuments(refreshDocuments);
  api.onToast(toast);
  api.onCommand(command => { if (command === 'new-space') openNewSpace(); });
  api.state().then(render);
}

setup();

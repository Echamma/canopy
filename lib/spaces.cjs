// Spaces are Canopy's projects. Each is a folder you can see on disk:
//   canopy-space.json   the space's name and color
//   Files/              shared files: what every app can import, and where exports land
//   Folio/              Folio's carousels for this space
//   .canopy/<app>/      each app's private data for this space (Forma's projects and media library, recovery copies)
// A small registry in Canopy's own data folder remembers which spaces exist, for the "Where to?" screen.
const fs = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');
const {safeFileName} = require('./files.cjs');

const COLORS = ['leaf', 'sky', 'clay', 'berry', 'moss'];
const MANIFEST = 'canopy-space.json';

function spacePaths(dir) {
  return {
    dir,
    manifest: path.join(dir, MANIFEST),
    files: path.join(dir, 'Files'),
    folio: path.join(dir, 'Folio'),
    appData: id => path.join(dir, '.canopy', id)
  };
}

async function writeJson(file, value) {
  await fs.mkdir(path.dirname(file), {recursive: true});
  const temporary = `${file}.${crypto.randomUUID()}.tmp`;
  try { await fs.writeFile(temporary, JSON.stringify(value, null, 2), 'utf8'); await fs.rename(temporary, file); }
  finally { await fs.rm(temporary, {force: true}); }
}
async function readJson(file) {
  try { return JSON.parse(await fs.readFile(file, 'utf8')); } catch { return null; }
}
const exists = file => fs.access(file).then(() => true, () => false);

function cleanName(name) {
  const value = String(name || '').replace(/\s+/g, ' ').trim().slice(0, 60);
  if (!value) throw new Error('Give your space a name.');
  return value;
}

function createSpaces({registryFile, defaultRoot}) {
  let queue = Promise.resolve();
  // Registry writes run one at a time so quick successive changes never overwrite each other.
  const serial = task => (queue = queue.catch(() => {}).then(task));

  async function load() {
    const data = await readJson(registryFile);
    return Array.isArray(data?.spaces) ? data.spaces.filter(space => space && typeof space.id === 'string' && typeof space.dir === 'string') : [];
  }
  const save = spaces => writeJson(registryFile, {version: 1, spaces});

  async function prepare(dir) {
    const paths = spacePaths(dir);
    await fs.mkdir(paths.files, {recursive: true});
    await fs.mkdir(paths.folio, {recursive: true});
    await fs.mkdir(path.join(dir, '.canopy'), {recursive: true});
    return paths;
  }

  async function list() {
    const spaces = await load();
    const result = [];
    for (const space of spaces) result.push({...space, available: await exists(spacePaths(space.dir).manifest)});
    return result.sort((a, b) => (b.lastOpenedAt || 0) - (a.lastOpenedAt || 0));
  }

  async function get(id) {
    return (await load()).find(space => space.id === id) || null;
  }

  const create = ({name, color, dir}) => serial(async () => {
    const clean = cleanName(name);
    const chosenColor = COLORS.includes(color) ? color : COLORS[0];
    let folder = dir;
    if (!folder) {
      const stem = safeFileName(clean);
      folder = path.join(defaultRoot, stem);
      for (let number = 2; await exists(folder); number++) folder = path.join(defaultRoot, `${stem} (${number})`);
    }
    if (await exists(spacePaths(folder).manifest)) throw new Error('That folder already holds a space. Open it instead.');
    const space = {id: crypto.randomUUID(), name: clean, color: chosenColor, createdAt: Date.now()};
    await prepare(folder);
    await writeJson(spacePaths(folder).manifest, {version: 1, ...space});
    const spaces = await load();
    const entry = {...space, dir: folder, lastOpenedAt: Date.now()};
    spaces.push(entry);
    await save(spaces);
    return entry;
  });

  // Adds an existing space folder (one made on another computer, or removed from the list earlier).
  const add = dir => serial(async () => {
    const manifest = await readJson(spacePaths(dir).manifest);
    if (!manifest || typeof manifest.id !== 'string' || typeof manifest.name !== 'string') throw new Error("That folder isn't a Canopy space.");
    const spaces = await load();
    const known = spaces.find(space => space.id === manifest.id);
    if (known) {
      known.dir = dir;
      known.name = manifest.name;
      await save(spaces);
      return known;
    }
    const entry = {id: manifest.id, name: manifest.name, color: COLORS.includes(manifest.color) ? manifest.color : COLORS[0], createdAt: manifest.createdAt || Date.now(), dir, lastOpenedAt: Date.now()};
    spaces.push(entry);
    await save(spaces);
    await prepare(dir);
    return entry;
  });

  const touch = id => serial(async () => {
    const spaces = await load();
    const space = spaces.find(item => item.id === id);
    if (!space) throw new Error('That space is no longer listed.');
    if (!(await exists(spacePaths(space.dir).manifest))) throw new Error(`The folder for ${space.name} wasn't found: ${space.dir}`);
    space.lastOpenedAt = Date.now();
    await save(spaces);
    await prepare(space.dir);
    return space;
  });

  const rename = (id, name) => serial(async () => {
    const clean = cleanName(name);
    const spaces = await load();
    const space = spaces.find(item => item.id === id);
    if (!space) throw new Error('That space is no longer listed.');
    space.name = clean;
    await save(spaces);
    const manifest = await readJson(spacePaths(space.dir).manifest);
    if (manifest) await writeJson(spacePaths(space.dir).manifest, {...manifest, name: clean});
    return space;
  });

  // Takes a space off the list; its folder and files stay where they are.
  const forget = id => serial(async () => {
    await save((await load()).filter(space => space.id !== id));
  });

  return {list, get, create, add, touch, rename, forget};
}

module.exports = {createSpaces, spacePaths, COLORS, MANIFEST};

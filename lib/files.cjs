// A space's Files folder: everything the apps share. Imports come from here, exports land here, and the sidebar lists it.
const fs = require('node:fs/promises');
const fsSync = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

const TYPES = {
  png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', webp: 'image/webp', gif: 'image/gif', svg: 'image/svg+xml',
  mp4: 'video/mp4', webm: 'video/webm', mov: 'video/quicktime', mkv: 'video/x-matroska', ogv: 'video/ogg',
  mp3: 'audio/mpeg', wav: 'audio/wav', m4a: 'audio/mp4', ogg: 'audio/ogg', flac: 'audio/flac',
  zip: 'application/zip', json: 'application/json', forma: 'application/x-forma-project', srt: 'text/plain', txt: 'text/plain'
};
// Files that are still being written (downloads, atomic saves) stay out of the list.
const HIDDEN = /^\.|\.(tmp|crdownload|part|download)$/i;

const typeOf = name => TYPES[path.extname(name).slice(1).toLowerCase()] || 'application/octet-stream';
function kindOf(name) {
  const lower = name.toLowerCase(), type = typeOf(name);
  if (lower.endsWith('.folio.json') || lower.endsWith('.forma')) return 'project';
  if (type.startsWith('image/')) return 'image';
  if (type.startsWith('video/')) return 'video';
  if (type.startsWith('audio/')) return 'audio';
  if (type === 'application/zip') return 'archive';
  return 'file';
}

// Turns any name into one Windows and Linux both accept, keeping its extension.
function safeFileName(name) {
  const cleaned = String(name || '').replace(/[<>:"/\\|?*\x00-\x1f]/g, '-').replace(/[. ]+$/, '').trim().slice(0, 150);
  return cleaned && !/^\.+$/.test(cleaned) ? cleaned : 'file';
}

// Resolves a name from the shell or an app to a path inside the folder, refusing anything that would leave it.
function resolveInside(dir, name) {
  if (typeof name !== 'string' || !name || path.basename(name) !== name || name === '.' || name === '..') throw new Error('Invalid file name.');
  return path.join(dir, name);
}

function* candidateNames(name) {
  const safe = safeFileName(name);
  const extension = safe.toLowerCase().endsWith('.folio.json') ? '.folio.json' : path.extname(safe);
  const stem = safe.slice(0, safe.length - extension.length) || 'file';
  yield safe;
  for (let number = 2; ; number++) yield `${stem} (${number})${extension}`;
}
async function uniqueName(dir, name) {
  for (const candidate of candidateNames(name)) {
    try { await fs.access(path.join(dir, candidate)); } catch { return candidate; }
  }
}
// Downloads must be given their path synchronously, so they use this form.
function uniqueNameSync(dir, name) {
  for (const candidate of candidateNames(name)) if (!fsSync.existsSync(path.join(dir, candidate))) return candidate;
}

async function listFiles(dir) {
  let entries;
  try { entries = await fs.readdir(dir, {withFileTypes: true}); }
  catch (error) { if (error.code === 'ENOENT') return []; throw error; }
  const files = [];
  for (const entry of entries) {
    if (!entry.isFile() || HIDDEN.test(entry.name)) continue;
    try {
      const stat = await fs.stat(path.join(dir, entry.name));
      files.push({name: entry.name, size: stat.size, modifiedAt: stat.mtimeMs, type: typeOf(entry.name), kind: kindOf(entry.name)});
    } catch {}
  }
  return files.sort((a, b) => b.modifiedAt - a.modifiedAt);
}

// Copies a file from anywhere on the computer into the folder; returns the name it was given.
async function copyInto(dir, source) {
  if (!(await fs.stat(source)).isFile()) throw new Error('Only files can be added.');
  await fs.mkdir(dir, {recursive: true});
  const name = await uniqueName(dir, path.basename(source));
  const temporary = path.join(dir, `.${crypto.randomUUID()}.tmp`);
  try { await fs.copyFile(source, temporary); await fs.rename(temporary, path.join(dir, name)); }
  finally { await fs.rm(temporary, {force: true}); }
  return name;
}

// Writes bytes an app produced (an export) into the folder; returns the name it was given.
async function writeInto(dir, name, data) {
  await fs.mkdir(dir, {recursive: true});
  const unique = await uniqueName(dir, name);
  const temporary = path.join(dir, `.${crypto.randomUUID()}.tmp`);
  try { await fs.writeFile(temporary, data); await fs.rename(temporary, path.join(dir, unique)); }
  finally { await fs.rm(temporary, {force: true}); }
  return unique;
}

module.exports = {listFiles, copyInto, writeInto, uniqueName, uniqueNameSync, safeFileName, resolveInside, typeOf, kindOf};

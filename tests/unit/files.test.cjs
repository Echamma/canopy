const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const files = require('../../lib/files.cjs');

const folder = async () => fs.mkdtemp(path.join(os.tmpdir(), 'canopy-files-'));

test('names are made safe and unique, keeping .folio.json together', async () => {
  const dir = await folder();
  assert.equal(files.safeFileName('a/b:c?.png'), 'a-b-c-.png');
  assert.equal(files.safeFileName('...'), 'file');
  await fs.writeFile(path.join(dir, 'cover.png'), '');
  await fs.writeFile(path.join(dir, 'story.folio.json'), '');
  assert.equal(await files.uniqueName(dir, 'cover.png'), 'cover (2).png');
  assert.equal(files.uniqueNameSync(dir, 'cover.png'), 'cover (2).png');
  assert.equal(await files.uniqueName(dir, 'story.folio.json'), 'story (2).folio.json');
  assert.equal(await files.uniqueName(dir, 'new.mp4'), 'new.mp4');
});

test('resolveInside refuses names that leave the folder', () => {
  assert.equal(files.resolveInside('C:\\space\\Files', 'clip.mp4'), path.join('C:\\space\\Files', 'clip.mp4'));
  for (const name of ['..', '../secret.txt', 'sub/clip.mp4', '', '.']) assert.throws(() => files.resolveInside('C:\\space\\Files', name));
});

test('listFiles hides partial downloads and temporary files, newest first', async () => {
  const dir = await folder();
  await fs.writeFile(path.join(dir, 'old.png'), 'x');
  await fs.utimes(path.join(dir, 'old.png'), new Date(2020, 1, 1), new Date(2020, 1, 1));
  await fs.writeFile(path.join(dir, 'new.mp4'), 'xx');
  for (const hidden of ['.secret', 'export.mp4.crdownload', 'x.tmp']) await fs.writeFile(path.join(dir, hidden), '');
  const list = await files.listFiles(dir);
  assert.deepEqual(list.map(file => file.name), ['new.mp4', 'old.png']);
  assert.equal(list[0].kind, 'video');
  assert.equal(list[1].type, 'image/png');
  assert.deepEqual(await files.listFiles(path.join(dir, 'missing')), []);
});

test('copyInto and writeInto never overwrite', async () => {
  const dir = await folder(), source = path.join(await folder(), 'clip.mp4');
  await fs.writeFile(source, 'video');
  assert.equal(await files.copyInto(dir, source), 'clip.mp4');
  assert.equal(await files.copyInto(dir, source), 'clip (2).mp4');
  assert.equal(await files.writeInto(dir, 'clip.mp4', Buffer.from('other')), 'clip (3).mp4');
  assert.equal(await fs.readFile(path.join(dir, 'clip.mp4'), 'utf8'), 'video');
});

test('kinds group what the sidebar shows', () => {
  assert.equal(files.kindOf('a.folio.json'), 'project');
  assert.equal(files.kindOf('a.forma'), 'project');
  assert.equal(files.kindOf('a.WAV'), 'audio');
  assert.equal(files.kindOf('a.zip'), 'archive');
  assert.equal(files.kindOf('notes.txt'), 'file');
});

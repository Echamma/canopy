const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const {createSpaces, spacePaths} = require('../../lib/spaces.cjs');

const made = [];
test.after(() => Promise.all(made.map(dir => fs.rm(dir, {recursive: true, force: true}))));
async function setup() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'canopy-spaces-'));
  made.push(root);
  return {root, spaces: createSpaces({registryFile: path.join(root, 'profile', 'spaces.json'), defaultRoot: path.join(root, 'Canopy')})};
}

test('a new space is a folder with its manifest, Files and Folio', async () => {
  const {root, spaces} = await setup();
  const space = await spaces.create({name: '  Launch   week ', color: 'clay'});
  assert.equal(space.name, 'Launch week');
  assert.equal(space.dir, path.join(root, 'Canopy', 'Launch week'));
  const paths = spacePaths(space.dir);
  for (const dir of [paths.files, paths.folio]) assert((await fs.stat(dir)).isDirectory());
  assert.deepEqual(JSON.parse(await fs.readFile(paths.manifest, 'utf8')).name, 'Launch week');
  assert.equal(paths.appData('forma'), path.join(space.dir, '.canopy', 'forma'));
});

test('names are required, colors fall back, and same-named spaces get their own folder', async () => {
  const {spaces} = await setup();
  await assert.rejects(spaces.create({name: '   '}), /name/);
  const first = await spaces.create({name: 'Client', color: 'purple'});
  const second = await spaces.create({name: 'Client'});
  assert.equal(first.color, 'leaf');
  assert.notEqual(first.dir, second.dir);
  assert.match(second.dir, /Client \(2\)$/);
});

test('list puts the most recently opened first and flags missing folders', async () => {
  const {spaces} = await setup();
  const a = await spaces.create({name: 'A'});
  const b = await spaces.create({name: 'B'});
  await new Promise(resolve => setTimeout(resolve, 5));
  await spaces.touch(a.id);
  assert.deepEqual((await spaces.list()).map(space => space.name), ['A', 'B']);
  await fs.rm(b.dir, {recursive: true});
  assert.equal((await spaces.list()).find(space => space.id === b.id).available, false);
  await assert.rejects(spaces.touch(b.id), /wasn't found/);
});

test('a space folder can be forgotten and added back, keeping its identity', async () => {
  const {spaces} = await setup();
  const space = await spaces.create({name: 'Portable'});
  await spaces.forget(space.id);
  assert.equal((await spaces.list()).length, 0);
  const added = await spaces.add(space.dir);
  assert.equal(added.id, space.id);
  await assert.rejects(spaces.add(path.dirname(space.dir)), /isn't a Canopy space/);
  await assert.rejects(spaces.create({name: 'Again', dir: space.dir}), /already holds a space/);
});

test('rename updates the list and the manifest', async () => {
  const {spaces} = await setup();
  const space = await spaces.create({name: 'Draft'});
  await spaces.rename(space.id, 'Final');
  assert.equal((await spaces.get(space.id)).name, 'Final');
  assert.equal(JSON.parse(await fs.readFile(spacePaths(space.dir).manifest, 'utf8')).name, 'Final');
});

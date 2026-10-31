const test = require('node:test');
const assert = require('node:assert/strict');
const {EventEmitter} = require('node:events');
const {createIpcRouter} = require('../../lib/ipc-router.cjs');

function fakeIpcMain() {
  const handlers = new Map(), listeners = new Map();
  return {
    handle: (channel, handler) => { if (handlers.has(channel)) throw new Error('duplicate ' + channel); handlers.set(channel, handler); },
    on: (channel, listener) => listeners.set(channel, listener),
    invoke: (channel, sender, ...args) => handlers.get(channel)({sender}, ...args),
    send: (channel, sender, ...args) => listeners.get(channel)({sender}, ...args)
  };
}
const page = id => Object.assign(new EventEmitter(), {id});

test('two apps can answer the same channel, each for its own page', async () => {
  const ipc = fakeIpcMain(), router = createIpcRouter(ipc);
  const forma = page(1), folio = page(2);
  router.claim(forma, 'forma');
  router.claim(folio, 'folio');
  router.scope('forma').handle('load-recovery', () => 'forma recovery');
  router.scope('folio').handle('load-recovery', () => 'folio recovery');
  assert.equal(await ipc.invoke('load-recovery', forma), 'forma recovery');
  assert.equal(await ipc.invoke('load-recovery', folio), 'folio recovery');
});

test('a page nobody claimed reaches no handler', () => {
  const ipc = fakeIpcMain(), router = createIpcRouter(ipc);
  router.scope('forma').handle('load-recovery', () => 'x');
  assert.throws(() => ipc.invoke('load-recovery', page(9)), /No handler/);
});

test('a scope cannot register a channel twice, but can after removing it', () => {
  const router = createIpcRouter(fakeIpcMain()), scope = router.scope('forma');
  scope.handle('save', () => 1);
  assert.throws(() => scope.handle('save', () => 2), /second handler/);
  scope.removeHandler('save');
  scope.handle('save', () => 3);
});

test('listeners only hear their own page, and removeAllListeners clears them', () => {
  const ipc = fakeIpcMain(), router = createIpcRouter(ipc);
  const forma = page(1), folio = page(2), heard = [];
  router.claim(forma, 'forma');
  router.claim(folio, 'folio');
  router.scope('forma').on('reply', (_, value) => heard.push('forma:' + value));
  router.scope('folio').on('reply', (_, value) => heard.push('folio:' + value));
  ipc.send('reply', forma, 1);
  ipc.send('reply', folio, 2);
  router.scope('forma').removeAllListeners('reply');
  ipc.send('reply', forma, 3);
  assert.deepEqual(heard, ['forma:1', 'folio:2']);
});

test('a destroyed page is forgotten', () => {
  const ipc = fakeIpcMain(), router = createIpcRouter(ipc), forma = page(1);
  router.claim(forma, 'forma');
  router.scope('forma').handle('ping', () => 'pong');
  forma.emit('destroyed');
  assert.throws(() => ipc.invoke('ping', forma), /No handler/);
});

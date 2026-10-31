// StreamCuts inside a space: `electron tests/streamcuts.cjs [screenshot.png]`. Starts StreamCuts' servers (or uses
// running ones), creates a project, sends a picture from the space, and checks it lands on the timeline. Slow on a
// first run, because StreamCuts' dev server compiles the editor.
const {app, desktopCapturer} = require('electron');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const assert = require('node:assert/strict');

const root = path.join(os.tmpdir(), 'canopy-streamcuts-' + process.pid);
const screenshot = process.argv[2];
process.env.CANOPY_TEST = '1';
process.env.CANOPY_SPACES_DIR = path.join(root, 'spaces');
app.setPath('userData', path.join(root, 'profile'));
require('../main.cjs');

// A 64×64 orange PNG, so StreamCuts has a real picture to import.
function png() {
  const zlib = require('node:zlib');
  const crc = buffer => { let c, crc = 0xffffffff; for (const byte of buffer) { c = (crc ^ byte) & 0xff; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; crc = (crc >>> 8) ^ c; } return (crc ^ 0xffffffff) >>> 0; };
  const chunk = (type, data) => { const length = Buffer.alloc(4); length.writeUInt32BE(data.length); const body = Buffer.concat([Buffer.from(type), data]); const sum = Buffer.alloc(4); sum.writeUInt32BE(crc(body)); return Buffer.concat([length, body, sum]); };
  const header = Buffer.alloc(13); header.writeUInt32BE(64, 0); header.writeUInt32BE(64, 4); header[8] = 8; header[9] = 2;
  const rows = Buffer.concat(Array.from({length: 64}, () => Buffer.concat([Buffer.from([0]), Buffer.alloc(64 * 3, Buffer.from([0xb8, 0x5a, 0x2e]))])));
  return Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk('IHDR', header), chunk('IDAT', zlib.deflateSync(rows)), chunk('IEND', Buffer.alloc(0))]);
}
const until = async (test, message, ms) => {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) { try { const value = await test(); if (value) return value; } catch {} await new Promise(resolve => setTimeout(resolve, 500)); }
  throw new Error('Timed out: ' + message);
};

app.whenReady().then(async () => {
  const {workspace, window} = global.__canopy;
  try {
    app.removeAllListeners('window-all-closed');
    if (screenshot) { window.setSize(1480, 920); window.show(); }
    const shell = window.webContents;
    if (shell.isLoading()) await new Promise(resolve => shell.once('did-finish-load', resolve));
    await shell.executeJavaScript(`window.canopy.createSpace({name: 'Stream night', color: 'berry'})`);
    const space = workspace.currentSpace();
    await fs.writeFile(path.join(space.dir, 'Files', 'thumbnail.png'), png());

    const started = Date.now();
    const instance = await workspace.openApp('streamcuts');
    console.log(`StreamCuts opened in ${Math.round((Date.now() - started) / 1000)} s`);
    const page = instance.view.webContents;
    assert.equal(await page.executeJavaScript('window.canopy.hosted && window.canopy.space'), 'Stream night');
    console.log('PASS: StreamCuts runs in the space and knows it');

    await until(() => page.executeJavaScript(`[...document.querySelectorAll('button')].some(b => b.textContent.includes('New project'))`), 'the projects page', 240000);
    await page.executeJavaScript(`[...document.querySelectorAll('button')].find(b => b.textContent.includes('New project')).click()`);
    await until(() => page.executeJavaScript(`location.pathname.startsWith('/editor/')`), 'the editor to open', 240000);
    await until(() => page.executeJavaScript(`document.readyState === 'complete' && !!document.querySelector('[data-timeline], canvas')`), 'the editor to load', 240000);
    await new Promise(resolve => setTimeout(resolve, 4000));
    console.log('PASS: A StreamCuts project opens in the pane');

    const result = await workspace.sendFiles(['thumbnail.png'], 'streamcuts');
    assert.equal(result.accepted, 1, 'StreamCuts took the pasted picture: ' + JSON.stringify(result));
    console.log('PASS: StreamCuts accepts a picture sent from the space (as a paste)');
    await until(() => page.executeJavaScript(`document.body.innerText.includes('thumbnail')`), 'the picture to show in StreamCuts', 30000);
    console.log('PASS: The picture shows up in StreamCuts');

    if (screenshot) {
      await new Promise(resolve => setTimeout(resolve, 1500));
      const [source] = (await desktopCapturer.getSources({types: ['window'], thumbnailSize: window.getContentBounds()})).filter(item => item.name.includes('Canopy'));
      await fs.writeFile(screenshot, source.thumbnail.toPNG());
    }
    await workspace.shutdown();
    console.log('PASS: StreamCuts servers Canopy started are stopped again');
    app.exit(0);
  } catch (error) {
    console.error(error);
    await workspace.shutdown().catch(() => {});
    app.exit(1);
  }
});

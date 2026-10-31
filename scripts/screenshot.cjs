// Captures the Canopy window in a few states for docs and design review: `electron scripts/screenshot.cjs <out-dir>`.
// Uses a throwaway profile and space folder, never your own spaces.
const {app, desktopCapturer} = require('electron');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');

const out = path.resolve(process.argv[2] || 'screenshots');
const root = path.join(os.tmpdir(), 'canopy-shots-' + process.pid);
process.env.CANOPY_TEST = '1';
process.env.CANOPY_SPACES_DIR = path.join(root, 'spaces');
app.setPath('userData', path.join(root, 'profile'));
require('../main.cjs');
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));

app.whenReady().then(async () => {
  try {
    const {workspace, window} = global.__canopy;
    await fs.mkdir(out, {recursive: true});
    window.setSize(1480, 920);
    window.center();
    window.show();
    const shell = window.webContents;
    if (shell.isLoading()) await new Promise(resolve => shell.once('did-finish-load', resolve));
    const shot = async name => {
      await wait(900);
      const [source] = await desktopCapturer.getSources({types: ['window'], thumbnailSize: window.getContentBounds(), fetchWindowIcons: false})
        .then(sources => sources.filter(item => item.name.includes('Canopy')));
      await fs.writeFile(path.join(out, name + '.png'), source.thumbnail.toPNG());
      console.log('Saved', name);
    };
    await shell.executeJavaScript(`window.canopy.createSpace({name: 'Client work', color: 'sky'})`);
    await workspace.closeSpace();
    await shell.executeJavaScript(`window.canopy.createSpace({name: 'Launch week', color: 'clay'})`);
    const space = workspace.currentSpace();
    for (const [name, size] of [['hero-loop.mp4', 48_000_000], ['cover.png', 820_000], ['voiceover.wav', 5_400_000]]) await fs.writeFile(path.join(space.dir, 'Files', name), Buffer.alloc(size));
    await workspace.closeSpace();
    await shot('1-where-to');
    await workspace.openSpace(space.id);
    await shot('2-space-launcher');
    await workspace.openApp('forma');
    await wait(1500);
    await shot('3-forma');
    workspace.setSplit(true);
    await workspace.openApp('folio');
    await wait(2500);
    await shot('4-forma-and-folio');
    const folio = (await workspace.openApp('folio')).view.webContents;
    await folio.executeJavaScript('exportSlides(true)');
    await wait(600);
    await shot('5-export-toast');
    await workspace.shutdown();
    app.exit(0);
  } catch (error) { console.error(error); app.exit(1); }
});

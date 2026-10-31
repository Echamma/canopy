// End-to-end check of a space with the real Forma and Folio: run with `electron tests/workspace.cjs`.
// StreamCuts needs its own servers, so it is only exercised with CANOPY_TEST_STREAMCUTS=1.
const {app} = require('electron');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const assert = require('node:assert/strict');

const root = path.join(os.tmpdir(), 'canopy-test-' + process.pid);
process.env.CANOPY_TEST = '1';
process.env.CANOPY_SPACES_DIR = path.join(root, 'spaces');
app.setPath('userData', path.join(root, 'profile'));
require('../main.cjs');

const PIXEL = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScL/nwAAAABJRU5ErkJggg==', 'base64');
const timeout = setTimeout(() => { console.error('Canopy workspace checks timed out'); app.exit(1); }, 180000);
const passed = [];
const check = (ok, message) => { assert(ok, message); passed.push(message); };
const until = async (test, message, ms = 30000) => {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) { const value = await test(); if (value) return value; await new Promise(resolve => setTimeout(resolve, 150)); }
  throw new Error('Timed out: ' + message);
};

app.whenReady().then(async () => {
  try {
    app.removeAllListeners('window-all-closed');
    const {workspace, window} = global.__canopy;
    const shell = window.webContents;
    if (shell.isLoading()) await new Promise(resolve => shell.once('did-finish-load', resolve));
    const inShell = code => shell.executeJavaScript(code);
    const viewFor = id => window.contentView.children.find(view => view.webContents && workspace.state().focused === id && view.getVisible());
    const contentsOf = async id => (await workspace.openApp(id)).view.webContents;

    check(workspace.state().screen === 'home', 'Canopy opens on the Where to? screen');
    await inShell(`window.canopy.createSpace({name: 'Launch week', color: 'clay'})`);
    const space = workspace.currentSpace();
    check(space?.name === 'Launch week' && workspace.state().screen === 'space', 'Creating a space opens it');
    for (const folder of ['Files', 'Folio', '.canopy']) await fs.access(path.join(space.dir, folder));
    check(JSON.parse(await fs.readFile(path.join(space.dir, 'canopy-space.json'), 'utf8')).name === 'Launch week', 'The space is a folder with a manifest, Files and Folio');

    await fs.writeFile(path.join(space.dir, 'Files', 'pixel.png'), PIXEL);
    await until(async () => (await inShell(`document.querySelectorAll('#files .row').length`)) === 1, 'the sidebar to list the file');
    check(true, 'The sidebar lists files added to the space folder');

    // Forma inside Canopy
    const forma = await contentsOf('forma');
    check(workspace.state().apps.find(item => item.id === 'forma').status === 'ready', 'Forma opens in a pane');
    check(await forma.executeJavaScript(`!!(window.formaDesktop.canopy && window.formaDesktop.canopy.space === 'Launch week')`), 'Forma knows it runs inside the space');
    await until(() => viewFor('forma'), 'the Forma view to be placed over its pane');
    check(true, 'The Forma view is shown over its pane');
    await forma.executeJavaScript(`window.FormaSession.ready.then(() => document.getElementById('new-empty').click())`);
    await until(() => forma.executeJavaScript(`!document.getElementById('projects-dialog').open`), 'Forma to start an empty project');
    const before = await forma.executeJavaScript(`window.FormaSession.snapshot().shapes.length`);
    const sent = await workspace.sendFiles(['pixel.png'], 'forma');
    check(sent.accepted === 1, 'Forma accepts a picture from the space');
    await until(async () => (await forma.executeJavaScript(`window.FormaSession.snapshot().shapes.length`)) === before + 1, 'the picture to become a Forma layer');
    check(await forma.executeJavaScript(`window.FormaSession.snapshot().shapes.at(-1).assetData.startsWith('forma-asset://media/')`), 'The picture is a Forma layer backed by its media library');
    await until(async () => (await workspace.listDocuments()).some(doc => doc.app === 'forma'), 'the Forma project to show in the sidebar');
    check(true, 'Forma projects appear under Projects');
    await fs.access(path.join(space.dir, '.canopy', 'forma', 'projects'));
    check(true, "Forma keeps this space's projects inside the space folder");

    // Downloads (Forma exports) land in Files
    await forma.executeJavaScript(`(() => { const a = document.createElement('a'); a.href = URL.createObjectURL(new Blob(['frame'], {type: 'text/plain'})); a.download = 'forma-frame.txt'; a.click(); })()`);
    await until(() => fs.access(path.join(space.dir, 'Files', 'forma-frame.txt')).then(() => true, () => false), 'the Forma export to land in Files');
    check(true, 'Exports from an app are saved to the space Files');

    // Folio inside Canopy, beside Forma
    workspace.setSplit(true);
    const folio = await contentsOf('folio');
    check(workspace.state().visible.length === 2 && workspace.state().visible.includes('forma'), 'Split view shows Forma and Folio side by side');
    await folio.executeJavaScript('window.FolioReady');
    check(await folio.executeJavaScript(`desktop.getProjectFolder()`) === path.join(space.dir, 'Folio'), "Folio's project folder is the space's Folio folder");
    check(await folio.executeJavaScript(`document.getElementById('change-project-folder').hidden`), 'Folio hides Change folder inside a space');
    const layersBefore = await folio.executeJavaScript(`(current().layers || []).length`);
    check((await workspace.sendFiles(['pixel.png'], 'folio')).accepted === 1, 'Folio accepts a picture from the space');
    await until(async () => (await folio.executeJavaScript(`(current().layers || []).length`)) === layersBefore + 1, 'the picture to become a Folio layer');
    check(true, 'The picture is added to the current Folio slide');
    await folio.executeJavaScript(`(async () => { await saveProject(); await exportSlides(true); })()`);
    await until(async () => (await fs.readdir(path.join(space.dir, 'Files'))).some(name => name.endsWith('.png') && name !== 'pixel.png'), 'the Folio export to land in Files');
    check(true, 'A Folio export goes straight to the space Files');
    await until(async () => (await workspace.listDocuments()).some(doc => doc.app === 'folio'), 'the Folio project to show in the sidebar');
    check((await fs.readdir(path.join(space.dir, 'Folio'))).some(name => name.endsWith('.folio.json')), 'Folio saves carousels in the space Folio folder');

    // Both apps answer their own copy of 'load-recovery' through the router.
    check(await forma.executeJavaScript(`window.formaDesktop.loadRecovery().then(project => Array.isArray(project?.shapes))`), "Forma's load-recovery reaches Forma");
    check(await folio.executeJavaScript(`desktop.loadRecovery().then(value => value === null || Array.isArray(value.slides))`), "Folio's load-recovery reaches Folio");

    // Moving between apps keeps each one running.
    workspace.setSplit(false);
    workspace.cycle(1);
    await until(() => workspace.state().visible.length === 1, 'one pane after closing split view');
    const formaId = forma.id;
    await workspace.openApp('forma');
    check(workspace.state().visible[0] === 'forma' && (await contentsOf('forma')).id === formaId, 'Switching back to Forma shows the same, still-running editor');

    // A second space has its own work; returning restores the first.
    await inShell(`window.canopy.createSpace({name: 'Side project', color: 'sky'})`);
    check(workspace.currentSpace().name === 'Side project' && workspace.state().visible.length === 0, 'A new space starts with no apps open');
    check((await workspace.listDocuments()).length === 0, "A new space doesn't show another space's projects");
    await workspace.openSpace(space.id);
    await until(() => workspace.state().apps.find(item => item.id === 'forma').status === 'ready', 'Forma to reopen with the space');
    check(workspace.state().visible.includes('forma'), 'Reopening a space brings back the apps you had open');
    const reopened = await contentsOf('forma');
    await until(() => reopened.executeJavaScript(`window.FormaSession.ready.then(() => window.FormaSession.snapshot().shapes.some(shape => shape.type === 'image'))`), "Forma to restore this space's project");
    check(true, "Forma restores this space's work after switching spaces");

    if (process.env.CANOPY_TEST_STREAMCUTS === '1') {
      const streamcuts = await contentsOf('streamcuts');
      check(await streamcuts.executeJavaScript(`window.canopy.hosted === true`), 'StreamCuts opens inside the space');
    }

    const failures = await workspace.shutdown();
    check(failures.length === 0, 'Every app saves when Canopy closes');
    clearTimeout(timeout);
    console.log(`PASS: ${passed.length} Canopy workspace checks with Forma and Folio.`);
    await fs.rm(root, {recursive: true, force: true}).catch(() => {});
    app.exit(0);
  } catch (error) {
    console.error(error);
    console.error('Passed before the failure:\n  ' + passed.join('\n  '));
    clearTimeout(timeout);
    app.exit(1);
  }
});

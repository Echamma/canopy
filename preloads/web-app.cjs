// Preload for web apps Canopy shows (StreamCuts). It tells the page it runs inside Canopy and delivers files from the
// space: each one is fetched from canopy-file:// and handed to the page as a paste, which StreamCuts imports and drops
// at the playhead. A page that wants files some other way can call window.canopy.saveFile to put its own output in
// the space's Files.
const {contextBridge, ipcRenderer} = require('electron');

const argument = process.argv.find(arg => arg.startsWith('--canopy='));
const info = argument ? JSON.parse(argument.slice('--canopy='.length)) : {};

contextBridge.exposeInMainWorld('canopy', {
  hosted: true,
  app: info.app,
  space: info.space,
  saveFile: async (name, blob) => ipcRenderer.invoke('canopy:web-save-file', String(name), new Uint8Array(await blob.arrayBuffer()))
});

const typable = element => !!element && (element.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(element.tagName));

ipcRenderer.on('canopy:deliver', async (_, {id, items}) => {
  try {
    const data = new DataTransfer();
    for (const item of items) {
      const response = await fetch(item.url);
      if (!response.ok) throw new Error(`${item.name} couldn't be read`);
      data.items.add(new File([await response.blob()], item.name, {type: item.type}));
    }
    // A paste into a text field would be read as text, so nothing must hold the keyboard focus.
    if (typable(document.activeElement)) document.activeElement.blur();
    const paste = new ClipboardEvent('paste', {clipboardData: data, bubbles: true, cancelable: true});
    window.dispatchEvent(paste);
    ipcRenderer.send('canopy:delivered', paste.defaultPrevented
      ? {id, ok: true}
      : {id, ok: false, reason: `Open a project in ${info.app === 'streamcuts' ? 'StreamCuts' : 'the app'} first, then send the file again.`});
  } catch (error) {
    ipcRenderer.send('canopy:delivered', {id, ok: false, reason: error.message});
  }
});

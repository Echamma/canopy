// Canopy's own window: the "Where to?" screen, the sidebar, tabs and pane chrome. Apps never see this API.
const {contextBridge, ipcRenderer, webUtils} = require('electron');

const call = channel => (...args) => ipcRenderer.invoke(channel, ...args);
const listen = channel => callback => {
  const listener = (_, value) => callback(value);
  ipcRenderer.on(channel, listener);
  return () => ipcRenderer.removeListener(channel, listener);
};

contextBridge.exposeInMainWorld('canopy', {
  state: call('canopy:state'),
  listSpaces: call('canopy:list-spaces'),
  defaultSpaceFolder: call('canopy:default-space-folder'),
  chooseSpaceFolder: call('canopy:choose-space-folder'),
  createSpace: call('canopy:create-space'),
  addSpaceFolder: call('canopy:add-space-folder'),
  openSpace: call('canopy:open-space'),
  goHome: call('canopy:go-home'),
  spaceMenu: call('canopy:space-menu'),
  revealSpace: call('canopy:reveal-space'),
  openApp: call('canopy:open-app'),
  focusApp: call('canopy:focus-app'),
  closeApp: call('canopy:close-app'),
  setSplit: call('canopy:set-split'),
  listFiles: call('canopy:list-files'),
  addFiles: call('canopy:add-files'),
  addDroppedFiles: list => ipcRenderer.invoke('canopy:add-paths', [...list].map(file => webUtils.getPathForFile(file)).filter(Boolean)),
  openFile: call('canopy:open-file'),
  sendFile: call('canopy:send-file'),
  fileMenu: call('canopy:file-menu'),
  startDrag: name => ipcRenderer.send('canopy:start-drag', name),
  listDocuments: call('canopy:list-documents'),
  openDocument: call('canopy:open-document'),
  layout: panes => ipcRenderer.send('canopy:layout', panes),
  setOverlay: value => ipcRenderer.send('canopy:overlay', value),
  onState: listen('canopy:state'),
  onFiles: listen('canopy:files'),
  onDocuments: listen('canopy:documents'),
  onToast: listen('canopy:toast'),
  onCommand: listen('canopy:command')
});

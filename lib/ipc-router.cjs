// Hosted apps register their IPC channels as if each had the main process to itself, and some share names (Forma and
// Folio both answer 'load-recovery'). Each app instance gets a scoped ipcMain look-alike; a call is routed to the
// instance that owns the page it came from.
function createIpcRouter(ipcMain) {
  const handlers = new Map(); // channel -> Map(owner -> handler)
  const listeners = new Map(); // channel -> Map(owner -> Set(listener))
  const owners = new Map(); // webContents id -> owner

  const ownerOf = event => owners.get(event.sender.id);

  function scope(owner) {
    const scoped = {
      handle(channel, handler) {
        if (!handlers.has(channel)) {
          handlers.set(channel, new Map());
          ipcMain.handle(channel, (event, ...args) => {
            const target = handlers.get(channel).get(ownerOf(event));
            if (!target) throw new Error(`No handler registered for '${channel}'`);
            return target(event, ...args);
          });
        }
        const byOwner = handlers.get(channel);
        if (byOwner.has(owner)) throw new Error(`Attempted to register a second handler for '${channel}'`);
        byOwner.set(owner, handler);
      },
      removeHandler(channel) {
        handlers.get(channel)?.delete(owner);
      },
      on(channel, listener) {
        if (!listeners.has(channel)) {
          listeners.set(channel, new Map());
          ipcMain.on(channel, (event, ...args) => {
            for (const target of listeners.get(channel).get(ownerOf(event)) || []) target(event, ...args);
          });
        }
        const byOwner = listeners.get(channel);
        if (!byOwner.has(owner)) byOwner.set(owner, new Set());
        byOwner.get(owner).add(listener);
        return scoped;
      },
      removeListener(channel, listener) {
        listeners.get(channel)?.get(owner)?.delete(listener);
        return scoped;
      },
      removeAllListeners(channel) {
        if (channel === undefined) for (const byOwner of listeners.values()) byOwner.delete(owner);
        else listeners.get(channel)?.delete(owner);
        return scoped;
      }
    };
    return scoped;
  }

  // Marks a page as belonging to an owner; IPC from it reaches only that owner's handlers.
  function claim(webContents, owner) {
    const id = webContents.id;
    owners.set(id, owner);
    webContents.once('destroyed', () => {
      if (owners.get(id) === owner) owners.delete(id);
    });
  }

  return {scope, claim, ownerOf};
}

module.exports = {createIpcRouter};

const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('api', {
  readCache: (n) => ipcRenderer.invoke('cache:read', n),
  loadPlaylist: (s) => ipcRenderer.invoke('playlist:load', s),
  loadEpg: (s) => ipcRenderer.invoke('epg:load', s),
  loadLib: (s) => ipcRenderer.invoke('lib:load', s),
  loadSeries: (s) => ipcRenderer.invoke('series:load', s),
  openFile: () => ipcRenderer.invoke('dialog:open'),
});

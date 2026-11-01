const { app, BrowserWindow, ipcMain, dialog, session } = require('electron');
const { Worker } = require('worker_threads');
const path = require('path');
const fs = require('fs');

const cacheDir = () => path.join(app.getPath('userData'), 'cache');
let win;

function createWindow() {
  win = new BrowserWindow({
    width: 1440, height: 860, minWidth: 960, minHeight: 600,
    backgroundColor: '#0b0d12', autoHideMenuBar: true, title: 'IPTV Player',
    webPreferences: { preload: path.join(__dirname, 'preload.js'), contextIsolation: true, backgroundThrottling: false },
  });
  win.setMenu(null);
  win.loadFile(path.join(__dirname, 'index.html'));
}

// Streams are served from arbitrary hosts; relax CORS for media requests only.
function setupCors() {
  session.defaultSession.webRequest.onHeadersReceived((d, cb) => {
    if (d.url.startsWith('file:')) return cb({});
    const h = { ...d.responseHeaders };
    for (const k of Object.keys(h)) if (k.toLowerCase().startsWith('access-control-')) delete h[k];
    h['Access-Control-Allow-Origin'] = ['*'];
    h['Access-Control-Allow-Headers'] = ['*'];
    cb({ responseHeaders: h });
  });
}

function runWorker(msg, onProgress) {
  return new Promise((resolve, reject) => {
    const w = new Worker(path.join(__dirname, 'worker.js'), { workerData: msg });
    w.on('message', (m) => (m.progress ? onProgress && onProgress(m.progress) : resolve(m.result)));
    w.on('error', reject);
    w.on('exit', (c) => c && reject(new Error('Worker exit ' + c)));
  });
}

async function readCache(name) {
  try { return JSON.parse(await fs.promises.readFile(path.join(cacheDir(), name + '.json'), 'utf8')); }
  catch { return null; }
}
async function writeCache(name, data) {
  await fs.promises.mkdir(cacheDir(), { recursive: true });
  await fs.promises.writeFile(path.join(cacheDir(), name + '.json'), JSON.stringify(data));
}

ipcMain.handle('cache:read', (_, name) => readCache(name));
ipcMain.handle('playlist:load', async (_, source) => {
  const r = await runWorker({ type: typeof source === 'object' ? 'xtream' : 'm3u', source });
  await writeCache('playlist', { ts: Date.now(), source, ...r });
  return r;
});
ipcMain.handle('lib:load', (_, src) => runWorker({ type: 'xtream-lib', source: src }));
ipcMain.handle('series:load', (_, src) => runWorker({ type: 'xtream-series', source: src }));
ipcMain.handle('epg:load', async (_, source) => {
  const r = await runWorker({ type: 'xmltv', source });
  await writeCache('epg', { ts: Date.now(), source, ...r });
  return r;
});
ipcMain.handle('dialog:open', async () => {
  const r = await dialog.showOpenDialog(win, {
    filters: [{ name: 'Playlists / EPG', extensions: ['m3u', 'm3u8', 'xml', 'gz', 'txt'] }],
    properties: ['openFile'],
  });
  return r.canceled ? null : r.filePaths[0];
});

app.whenReady().then(() => { setupCors(); createWindow(); });
app.on('window-all-closed', () => app.quit());

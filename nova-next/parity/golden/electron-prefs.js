// The 1.7 reference for the Settings pages: the repo's own prefs/index.html and prefs/preload.js in
// the same window prefs-window.js makes, answered with the data the Rust app gave its own Settings
// page (record.json from nova-island --prefs-pages), so the only difference left is the rendering.
//   electron.exe electron-prefs.js <dir>
// Reads <dir>/record.json, saves <dir>/<page>-ref.png for each page, logs to <dir>/electron-prefs.log.
const { app, BrowserWindow, ipcMain } = require('electron');
const path = require('path');
const fs = require('fs');

const dir = process.argv[process.argv.findIndex((a) => a.endsWith('electron-prefs.js')) + 1];
const ROOT = path.join(__dirname, '..', '..', '..');
const PAGES = ['general', 'island', 'themes-page', 'companion', 'faith', 'alerts', 'connections', 'plugins', 'support', 'more'];
const logFile = path.join(dir, 'electron-prefs.log');
const log = (s) => fs.appendFileSync(logFile, `${new Date().toISOString()} ${s}\n`);
process.on('uncaughtException', (e) => { log(`crash ${e.stack}`); app.exit(2); });

// the last answer the Rust app gave for each channel
const answers = {};
for (const { c, v } of JSON.parse(fs.readFileSync(path.join(dir, 'record.json'), 'utf8'))) answers[c] = v;
for (const c of Object.keys(answers)) ipcMain.handle(c, () => answers[c]);

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

app.whenReady().then(async () => {
  fs.writeFileSync(logFile, '');
  const win = new BrowserWindow({
    width: 760, height: 560, frame: false, show: false, backgroundColor: '#0f0f15',
    webPreferences: { preload: path.join(ROOT, 'prefs', 'preload.js'), contextIsolation: true, sandbox: true, backgroundThrottling: false },
  });
  win.webContents.on('console-message', (_e, _l, m) => log(`console ${m}`));
  await win.loadFile(path.join(ROOT, 'prefs', 'index.html'));
  win.showInactive();
  await wait(3000);
  for (const id of PAGES) {
    await win.webContents.executeJavaScript(`document.querySelector('[data-page="${id}"]').click(); window.scrollTo(0, 0); 1`);
    await wait(900);
    const img = await win.webContents.capturePage();
    fs.writeFileSync(path.join(dir, `${id}-ref.png`), img.toPNG());
    log(`${id}: ok ${img.getSize().width}x${img.getSize().height}`);
  }
  app.exit(0);
});

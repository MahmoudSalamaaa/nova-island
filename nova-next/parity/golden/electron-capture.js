// The 1.7 reference for golden screenshots: the staged island rendered by the repo's own Electron
// (the same engine and GPU path as the shipped app), in a 640x320 window like the island window.
//   electron.exe electron-capture.js <port> <outDir> <state>=<ms> [<state>=<ms> ...]
// (a state can carry more of the query: music&theme=lava&n=music-lava=6800)
// For each state: load http://127.0.0.1:<port>/promo/src/island.html?state=<state>&bd=<bd>, wait <ms>,
// save <outDir>/<state>-ref.png. Progress and errors go to <outDir>/electron-capture.log.
const { app, BrowserWindow } = require('electron');
const path = require('path');
const fs = require('fs');

const args = process.argv.slice(process.argv.findIndex((a) => a.endsWith('electron-capture.js')) + 1);
const [port, outDir, ...jobs] = args;
const BD = 'promo/src/out/01-hero-bd0.png';
const FREEZE = "document.getAnimations().forEach((a) => { try { a.pause(); a.currentTime = 0; } catch {} }); new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))";
const logFile = path.join(outDir, 'electron-capture.log');
const log = (s) => fs.appendFileSync(logFile, `${new Date().toISOString()} ${s}\n`);

app.commandLine.appendSwitch('autoplay-policy', 'no-user-gesture-required');
// one window per state: closing one must not end the run
app.on('window-all-closed', () => {});
process.on('uncaughtException', (e) => { log(`crash ${e.stack}`); app.exit(2); });

app.whenReady().then(async () => {
  fs.writeFileSync(logFile, '');
  for (const job of jobs) {
    const eq = job.lastIndexOf('=');
    const [query, ms] = [job.slice(0, eq), job.slice(eq + 1)];
    // n=<name> names the picture (variants of one state: themes, vertical)
    const state = new URLSearchParams(`state=${query}`).get('n') || query.split('&')[0];
    try {
      const win = new BrowserWindow({
        width: 640, height: 320, x: 0, y: 600, frame: false, resizable: false, show: false, useContentSize: true,
        backgroundColor: '#1a1a24', paintWhenInitiallyHidden: true,
        webPreferences: { backgroundThrottling: false },
      });
      win.webContents.on('render-process-gone', (_e, d) => log(`${state}: renderer gone ${d.reason}`));
      await win.loadURL(`http://127.0.0.1:${port}/promo/src/island.html?state=${query}&bd=${BD}`);
      win.showInactive();
      await new Promise((r) => setTimeout(r, Number(ms)));
      // freeze=1: every animation stopped at its start, so only the drawing is compared
      if (/[?&]freeze=1/.test(`&${query}`)) await win.webContents.executeJavaScript(FREEZE);
      const img = await win.webContents.capturePage();
      fs.writeFileSync(path.join(outDir, `${state}-ref.png`), img.toPNG());
      log(`${state}: ok ${img.getSize().width}x${img.getSize().height}`);
      win.destroy();
    } catch (e) {
      log(`${state}: ${e.stack}`);
    }
  }
  app.exit(0);
});

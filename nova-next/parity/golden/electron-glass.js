// Step 5's reference: the glass capture exactly as nova.js captureBackdrop() does it (desktopCapturer
// thumbnail of the display at 1/3, cropped to the window's bounds), for the 640x320 area at (x, y).
//   electron.exe electron-glass.js x y out.bgra   -> "w h\n" + raw BGRA
const { app, desktopCapturer, screen } = require('electron');
const fs = require('fs');

const args = process.argv.slice(process.argv.findIndex((a) => a.endsWith('electron-glass.js')) + 1);
const [x, y, out] = [Number(args[0]), Number(args[1]), args[2]];
app.whenReady().then(async () => {
  const b = { x, y, width: 640, height: 320 };
  const display = screen.getDisplayMatching(b);
  const scale = 1 / 3;
  const sources = await desktopCapturer.getSources({ types: ['screen'], thumbnailSize: { width: Math.round(display.bounds.width * scale), height: Math.round(display.bounds.height * scale) } });
  const src = sources.find((s) => s.display_id === String(display.id)) || sources[0];
  const k = src.thumbnail.getSize().width / display.bounds.width;
  const crop = src.thumbnail.crop({ x: Math.round((b.x - display.bounds.x) * k), y: Math.round((b.y - display.bounds.y) * k), width: Math.round(b.width * k), height: Math.round(b.height * k) });
  const s = crop.getSize();
  fs.writeFileSync(out, Buffer.concat([Buffer.from(`${s.width} ${s.height}\n`), crop.toBitmap()]));
  app.exit(0);
});

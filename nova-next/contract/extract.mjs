// Lists every channel the three pages use to talk to the app, read from the unchanged preload files
// in the repo root, and writes contract/ipc.json. The Rust router is checked against this list, so a
// channel added to a preload file later fails the check until 1.8 handles it too.
//   node nova-next/contract/extract.mjs          write ipc.json
//   node nova-next/contract/extract.mjs --check  exit 1 if ipc.json is out of date
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..', '..');
const SOURCES = [
  { bridge: 'nova', file: 'preload.js' },
  { bridge: 'prefs', file: 'prefs/preload.js' },
  { bridge: 'mini', file: 'mini/preload.js' },
];

function channels(src) {
  const found = [];
  // on('channel') helpers and direct ipcRenderer.on('channel', ...) listeners: main -> page
  for (const m of src.matchAll(/\bon\('([\w-]+)'\)|ipcRenderer\.on\('([\w-]+)'/g)) found.push({ kind: 'event', channel: m[1] || m[2] });
  // ipcRenderer.send / invoke: page -> main
  for (const m of src.matchAll(/ipcRenderer\.(send|invoke)\('([\w-]+)'/g)) found.push({ kind: m[1], channel: m[2] });
  return found;
}

const out = SOURCES.map(({ bridge, file }) => {
  const src = fs.readFileSync(path.join(root, file), 'utf8');
  const list = channels(src);
  const seen = new Set();
  const uniq = list.filter((c) => {
    const k = `${c.kind}:${c.channel}`;
    if (seen.has(k)) return false;
    seen.add(k);
    return true;
  }).sort((a, b) => a.kind.localeCompare(b.kind) || a.channel.localeCompare(b.channel));
  return { bridge, file, channels: uniq };
});

const json = `${JSON.stringify(out, null, 2)}\n`;
const target = path.join(here, 'ipc.json');
if (process.argv.includes('--check')) {
  const now = fs.existsSync(target) ? fs.readFileSync(target, 'utf8') : '';
  if (now !== json) {
    console.error('contract/ipc.json is out of date: a preload file changed. Run extract.mjs and handle the new channels.');
    process.exit(1);
  }
  console.log('contract up to date');
} else {
  fs.writeFileSync(target, json);
  for (const b of out) {
    const n = (k) => b.channels.filter((c) => c.kind === k).length;
    console.log(`${b.bridge}: ${b.channels.length} channels (${n('invoke')} invoke, ${n('send')} send, ${n('event')} event)`);
  }
}

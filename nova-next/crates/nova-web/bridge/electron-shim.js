// Runs the app's own preload file (preload.js, prefs/preload.js, mini/preload.js) unchanged inside
// WebView2. The preload calls require('electron') and gets this stand-in, which carries Electron's
// IPC over chrome.webview messages. The pages therefore see exactly the API they see in 1.7.
//   page -> app  {t:'snd', c, a}      send
//                {t:'inv', i, c, a}   invoke (the app answers with {t:'res', i, ok, v})
//   app -> page  {t:'ev', c, d}       event for ipcRenderer.on(c)
// Files from webUtils.getPathForFile travel as the real File objects; the app reads their paths.
// Glass frames arrive as shared memory and reach the 'backdrop' listeners as {w, h, bgra}.
const wv = window.chrome.webview;
const listeners = new Map();
const pending = new Map();
let nextId = 1;
const files = [];
const TOKEN = 'nova-file:';

function post(msg, args) {
  const objs = [];
  const walk = (v) => {
    if (Array.isArray(v)) return v.map(walk);
    if (typeof v === 'string' && v.startsWith(TOKEN)) {
      objs.push(files[Number(v.slice(TOKEN.length))]);
      return { $file: objs.length - 1 };
    }
    return v;
  };
  msg.a = walk(args);
  if (objs.length) wv.postMessageWithAdditionalObjects(msg, objs);
  else wv.postMessage(msg);
}

const ipcRenderer = {
  send(channel, ...args) {
    post({ t: 'snd', c: channel }, args);
  },
  invoke(channel, ...args) {
    const i = nextId++;
    return new Promise((ok, no) => {
      pending.set(i, { ok, no });
      post({ t: 'inv', i, c: channel }, args);
    });
  },
  on(channel, fn) {
    if (!listeners.has(channel)) listeners.set(channel, []);
    listeners.get(channel).push(fn);
    return ipcRenderer;
  },
};
const event = { sender: ipcRenderer };

wv.addEventListener('message', (e) => {
  const m = e.data;
  if (!m || typeof m !== 'object') return;
  if (m.t === 'ev') {
    for (const fn of listeners.get(m.c) || []) fn(event, m.d);
  } else if (m.t === 'res') {
    const p = pending.get(m.i);
    if (!p) return;
    pending.delete(m.i);
    if (m.ok) p.ok(m.v);
    else p.no(new Error(m.v));
  }
});

// a writable copy, like the Buffer Electron hands over (renderer.js swaps the bytes in place)
wv.addEventListener('sharedbufferreceived', (e) => {
  const buf = e.getBuffer();
  const d = e.additionalData || {};
  const bgra = new Uint8Array(buf.slice(0));
  wv.releaseBuffer(buf);
  for (const fn of listeners.get(d.c || 'backdrop') || []) fn(event, { w: d.w, h: d.h, bgra });
});

function deepFreeze(o) {
  if (o && typeof o === 'object') {
    for (const v of Object.values(o)) deepFreeze(v);
    Object.freeze(o);
  }
  return o;
}

const contextBridge = {
  exposeInMainWorld(key, api) {
    Object.defineProperty(window, key, { value: deepFreeze(api), enumerable: true });
  },
};

const webUtils = {
  getPathForFile(f) {
    if (!(f instanceof File)) throw new TypeError('getPathForFile needs a File');
    files.push(f);
    return TOKEN + (files.length - 1);
  },
};

const require = (name) => (name === 'electron' ? { contextBridge, ipcRenderer, webUtils } : undefined);

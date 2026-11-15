// node cdp.mjs <port> <url-part> <out-dir> <step>... ; a step is "js:<expression>" (awaited),
// "wait:<ms>" or "shot:<name>" (a PNG of the page)
import { writeFileSync } from 'node:fs';
const [port, part, out, ...steps] = process.argv.slice(2);
const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const t = list.find((x) => x.type === 'page' && x.url.includes(part));
if (!t) { console.log('no page with', part, list.map((x) => x.url)); process.exit(1); }
const ws = new WebSocket(t.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener('open', r));
let id = 0;
const waiting = new Map();
ws.addEventListener('message', (e) => {
  const m = JSON.parse(e.data);
  if (m.id && waiting.has(m.id)) { waiting.get(m.id)(m); waiting.delete(m.id); }
});
const call = (method, params = {}) => new Promise((r) => { const n = ++id; waiting.set(n, r); ws.send(JSON.stringify({ id: n, method, params })); });
for (const s of steps) {
  if (s.startsWith('js:')) {
    const r = await call('Runtime.evaluate', { expression: s.slice(3), awaitPromise: true, returnByValue: true });
    console.log('js =>', JSON.stringify(r.result?.result?.value ?? r.result?.exceptionDetails?.exception?.description ?? r.result).slice(0, 600));
  } else if (s.startsWith('wait:')) {
    await new Promise((r) => setTimeout(r, +s.slice(5)));
  } else if (s.startsWith('shot:')) {
    const r = await call('Page.captureScreenshot', { format: 'png' });
    writeFileSync(`${out}/${s.slice(5)}.png`, Buffer.from(r.result.data, 'base64'));
    console.log('shot', s.slice(5));
  }
}
ws.close();

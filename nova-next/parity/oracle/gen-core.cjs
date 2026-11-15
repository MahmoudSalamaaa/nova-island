// References for nova-core, produced by the current settings.js and features.js themselves:
//   json.json      values and the exact text settings.save() writes for them (JSON.stringify(v, null, 2))
//   features.json  random patches and what features.pick() keeps; the file text after set() and reset()
//   node nova-next/parity/oracle/gen-core.cjs
const { userData } = require('./electron-stub.cjs');
const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '..', '..', '..');
const out = path.join(__dirname, '..', 'fixtures');
fs.mkdirSync(out, { recursive: true });

// a seeded generator so the references never change between runs
let seed = 20261103;
const rnd = () => ((seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648);
const pickOne = (a) => a[Math.floor(rnd() * a.length)];

// ---- JSON text ----
const settings = require(path.join(root, 'settings.js'));
const numbers = [0, -0, 1, -1, 5, 5.5, 0.1, 0.2 + 0.1, 1 / 3, 1e21, 1e-7, 123456789012, 1.5e-7, 2 ** 53, -1e-300, 1e300, 0.000001, 1e20, 4.35, 100.5];
const strings = ['', 'a', 'ä ünï', 'مواقيت الصلاة', 'quote " back \\ slash /', 'line\nbreak\ttab\r\b\f', '\u0001\u001f\u007f', '😀 emoji', '\u2028\u2029'];
const values = [
  ...numbers,
  ...strings,
  true, false, null,
  [], {}, [1, [2, [3]]],
  { b: 1, a: 2, 10: 'ten', 2: 'two', '-1': 'neg', '01': 'lead', x: { y: [] } },
  { ax: 812.5, ay: 8, docked: false, vertical: false, side: 'right', placed: true },
  [{ path: 'C:\\Users\\me\\Desktop\\a.png', until: 1793650000000 }, { path: 'D:\\x.zip' }],
];
const json = values.map((v) => {
  settings.save('t.json', v);
  return { value: v === undefined ? null : v, text: fs.readFileSync(path.join(userData, 't.json'), 'utf8') };
});
// the values as JS sees them, so -0 / big numbers don't change on the way through JSON
fs.writeFileSync(path.join(out, 'json.json'), JSON.stringify(json.map((j) => ({ text: j.text })), null, 1));

// ---- features ----
const features = require(path.join(root, 'features.js'));
const keys = Object.keys(features.DEFAULTS);
// (not '__proto__', 'constructor', 'toString'...: features.js uses `k in DEFAULTS`, which sees Object.prototype
// names as known keys and throws; 1.8 treats them as unknown keys)
const junk = ['', 'x', 'theme', 'TabMedia', '1'];
const anyValue = () => pickOne([
  true, false, 0, 1, -5, 2.4999, 2.5, -2.5, 999999, 1e-9, 7.5, 'original', 'nody', 'pink', '5', null, [], {}, 1e400, -0,
  Math.round(rnd() * 3000) - 500, rnd() * 400 - 50,
]);
const patches = [];
for (let i = 0; i < 4000; i++) {
  const p = {};
  const n = 1 + Math.floor(rnd() * 6);
  for (let j = 0; j < n; j++) p[rnd() < 0.85 ? pickOne(keys) : pickOne(junk)] = anyValue();
  patches.push(p);
}
const readFile = (f) => { try { return fs.readFileSync(path.join(userData, f), 'utf8'); } catch { return ''; } };
const picked = patches.map((p) => {
  const before = features.get();
  const after = features.set(p);
  return { patch: JSON.stringify(p), changed: JSON.stringify(after) !== JSON.stringify(before), file: readFile('features.json') };
});
features.reset();
const resetText = fs.readFileSync(path.join(userData, 'features.json'), 'utf8');
fs.writeFileSync(path.join(out, 'features.json'), JSON.stringify({ defaults: features.DEFAULTS, solid: features.SOLID_THEMES, resetText, steps: picked }));
console.log(`fixtures: json ${json.length} values, features ${picked.length} patches`);
fs.rmSync(userData, { recursive: true, force: true });

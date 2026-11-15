// References for nova-services::plugins: what plugins.js's checkBundle makes of the real Quran
// Radio plugin and of tampered copies (a station on a foreign host, a bad id, a changed byte).
// plugins.js keeps checkBundle private, so it is pulled out of the file text and run as is.
require('./electron-stub.cjs');
const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '..', '..', '..');
const src = fs.readFileSync(path.join(root, 'plugins.js'), 'utf8');
const signing = require(path.join(root, 'signing.js'));
// the pure part of plugins.js: constants and checks, up to the file-system code
const body = src.slice(src.indexOf('const CATALOG_URL'), src.indexOf('function loadInstalled'))
  .replace(/^const dir = .*$/m, '').replace(/^let state = .*$/m, '').replace(/^let cache = .*$/m, '');
// eslint-disable-next-line no-new-func
const checkBundle = new Function('signing', `${body}; return checkBundle;`)(signing);

const nvp = JSON.parse(fs.readFileSync(path.join(root, 'nova-island-plugins', 'dist', 'quran-radio-1.0.0.nvp'), 'utf8'));
const clone = (x) => JSON.parse(JSON.stringify(x));
const cases = { real: nvp };
const foreign = clone(nvp); foreign.manifest.stations[0].url = 'https://evil.example.com/stream'; cases.foreignHost = foreign;
const badId = clone(nvp); badId.manifest.id = 'Quran_Radio'; cases.badId = badId;
const changed = clone(nvp); changed.manifest.name += '!'; cases.tampered = changed;
const out = {};
for (const [k, b] of Object.entries(cases)) out[k] = { bundle: b, manifest: checkBundle(b) };
fs.writeFileSync(path.join(__dirname, '..', 'fixtures', 'plugins.json'), JSON.stringify(out));
console.log(Object.entries(out).map(([k, v]) => `${k}: ${v.manifest ? 'accepted' : 'refused'}`).join(', '));

// References for nova-sign, produced by the current signing.js:
//   canonical  the canonical text of real signed files and of tricky made-up objects
//   verify     real news items, the plugin catalog and plugin bundles, plus tampered copies and
//              signatures written differently (spaces, URL-safe letters), with signing.js's answer
//   node nova-next/parity/oracle/gen-sign.cjs
require('./electron-stub.cjs');
const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '..', '..', '..');
const { canonical, verify } = require(path.join(root, 'signing.js'));
const out = path.join(__dirname, '..', 'fixtures');

const signed = [];
for (const item of JSON.parse(fs.readFileSync(path.join(root, 'news.json'), 'utf8'))) {
  const { sig, ...body } = item;
  signed.push({ what: `news ${item.id}`, body, sig });
}
for (const file of ['plugins-catalog.json', 'nova-island-plugins/plugins.json']) {
  const cat = JSON.parse(fs.readFileSync(path.join(root, file), 'utf8'));
  signed.push({ what: file, body: { plugins: cat.plugins }, sig: cat.sig });
}
const nvpDir = path.join(root, 'nova-island-plugins', 'dist');
for (const f of fs.existsSync(nvpDir) ? fs.readdirSync(nvpDir) : []) {
  const { manifest, sig } = JSON.parse(fs.readFileSync(path.join(nvpDir, f), 'utf8'));
  signed.push({ what: f, body: manifest, sig });
}

const verifyCases = [];
for (const s of signed) {
  verifyCases.push({ what: s.what, body: s.body, sig: s.sig });
  verifyCases.push({ what: `${s.what} spaced sig`, body: s.body, sig: s.sig.replace(/(.{8})/g, '$1 ') });
  verifyCases.push({ what: `${s.what} url-safe sig`, body: s.body, sig: s.sig.replace(/\+/g, '-').replace(/\//g, '_') });
  verifyCases.push({ what: `${s.what} no padding`, body: s.body, sig: s.sig.replace(/=+$/, '') });
  const tampered = JSON.parse(JSON.stringify(s.body));
  const k = Object.keys(tampered)[0];
  tampered[k] = typeof tampered[k] === 'string' ? `${tampered[k]}!` : 'changed';
  verifyCases.push({ what: `${s.what} tampered`, body: tampered, sig: s.sig });
  verifyCases.push({ what: `${s.what} bad sig`, body: s.body, sig: `A${s.sig.slice(1)}` });
  verifyCases.push({ what: `${s.what} short sig`, body: s.body, sig: s.sig.slice(0, 40) });
}
for (const c of verifyCases) c.ok = verify(c.body, c.sig);

const tricky = [
  { b: 1, a: [1, { d: 2, c: null }], é: 'x', Z: true, 10: 2.5, 2: -0, '': '', 'a b': 1e21 },
  { '😀': 1, 'ﬀ': 2, 'z': 3, 'Ω': 4 },
  ['مواقيت', 'quote " \\ /', 'line\nbreak', 1.5e-7, 0.1 + 0.2],
  { nested: { deep: { deeper: [[], {}] } } },
];
const canon = [...signed.map((s) => s.body), ...tricky].map((v) => ({ value: JSON.stringify(v), canonical: canonical(v) }));

fs.writeFileSync(path.join(out, 'sign.json'), JSON.stringify({ canonical: canon, verify: verifyCases }, null, 1));
console.log(`fixtures: ${canon.length} canonical, ${verifyCases.length} verify (${verifyCases.filter((c) => c.ok).length} valid)`);

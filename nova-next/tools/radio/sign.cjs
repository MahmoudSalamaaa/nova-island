// Signs nova-next/plugins/quran-radio/manifest.json with the Nova key (keys/, never committed) into
// nova-next/plugins/quran-radio.nvp: {manifest, sig}, the same bundle as every Nova plugin.
//   node nova-next/tools/radio/sign.cjs
const crypto = require('crypto');
const fs = require('fs');
const path = require('path');

const ROOT = path.join(__dirname, '..', '..', '..');
const { canonical, verify } = require(path.join(ROOT, 'signing.js'));
const key = crypto.createPrivateKey(fs.readFileSync(path.join(ROOT, 'keys', 'nova-signing-private.pem')));
const dir = path.join(ROOT, 'nova-next', 'plugins');
const manifest = JSON.parse(fs.readFileSync(path.join(dir, 'quran-radio', 'manifest.json'), 'utf8'));
const sig = crypto.sign(null, Buffer.from(canonical(manifest)), key).toString('base64');
if (!verify(manifest, sig)) throw new Error('the signature does not check out against the public key');
fs.writeFileSync(path.join(dir, 'quran-radio.nvp'), JSON.stringify({ manifest, sig }));
console.log(`signed quran-radio ${manifest.version}, ${manifest.stations.length} stations`);

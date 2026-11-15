// Signs nova-next/plugins/tasks/manifest.json with the Nova key (keys/, never committed) into
// nova-next/plugins/tasks.nvp: {manifest, sig}, the same bundle as every Nova plugin.
//   node nova-next/tools/tasks/sign.cjs
const crypto = require('crypto');
const fs = require('fs');
const path = require('path');

const ROOT = path.join(__dirname, '..', '..', '..');
const { canonical, verify } = require(path.join(ROOT, 'signing.js'));
const key = crypto.createPrivateKey(fs.readFileSync(path.join(ROOT, 'keys', 'nova-signing-private.pem')));
const dir = path.join(ROOT, 'nova-next', 'plugins');
const manifest = JSON.parse(fs.readFileSync(path.join(dir, 'tasks', 'manifest.json'), 'utf8'));
const sig = crypto.sign(null, Buffer.from(canonical(manifest)), key).toString('base64');
if (!verify(manifest, sig)) throw new Error('the signature does not check out against the public key');
fs.writeFileSync(path.join(dir, 'tasks.nvp'), JSON.stringify({ manifest, sig }));
console.log(`signed tasks ${manifest.version}`);

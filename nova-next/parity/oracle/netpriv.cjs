// 1.7's own answer for the network link and the mic / camera dots, right now (network.js linkKind,
// privacy.js query + parse), to compare with nova-services' netpriv example.
const os = require('os');
const path = require('path');
const { execFile } = require('child_process');
const p = require(path.resolve(__dirname, '..', '..', '..', 'privacy.js'));

let link = 'none';
for (const [name, addrs] of Object.entries(os.networkInterfaces())) {
  if (!(addrs || []).some((a) => a.family === 'IPv4' && !a.internal && !a.address.startsWith('169.254.'))) continue;
  if (/wi-?fi|wlan|wireless/i.test(name)) { link = 'wifi'; break; }
  link = 'ethernet';
}
const BASE = String.raw`HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore`;
const q = (d) => new Promise((r) => execFile('reg.exe', ['query', `${BASE}\\${d}`, '/s', '/v', 'LastUsedTimeStop'], { windowsHide: true }, (_e, o) => r(p.parse(String(o || '')))));
Promise.all([q('microphone'), q('webcam')]).then(([mic, cam]) => console.log(JSON.stringify({ link, mic, cam })));

// References for the smart clipboard, produced by the current smartclip.js: hand-picked cases
// (links, Arabic domains, colours, dates and phone numbers that must not be maths, odd spaces) plus
// 6000 generated arithmetic expressions. classify() throwing (an invalid link) is recorded too.
//   node nova-next/parity/oracle/gen-smartclip.cjs
const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '..', '..', '..');
const { classify } = require(path.join(root, 'smartclip.js'));
const out = path.join(__dirname, '..', 'fixtures');

const long = (n) => Array.from({ length: n }, (_, i) => `word${i}`).join(' ');
const hand = [
  '', '   ', 'hello', 'https://www.example.com/a?b=c#d', 'HTTP://WWW.Example.COM:8080/x', 'https://user:pw@sub.example.org/',
  'https://مثال.مصر/صفحة', 'https://bücher.de', 'https://例え.テスト/', 'http://[::1]:8080/', 'https://192.168.1.1/admin',
  'http://[', 'http://', 'https://a<b.com', 'https://example.com:99999', 'https://exa mple.com', ' https://padded.com \n',
  'ftp://not-a-web-link', 'https://' + 'a'.repeat(2000), 'www.example.com',
  '#fff', '#FFFF', '#a1b2c3', '#A1B2C3D4', '#abcde', '#ggg', 'rgb(255, 0, 0)', 'RGBA(1,2,3,.5)', 'hsl(120deg 50% 50%)', 'hsla(1, 2%, 3%, 0.4)', 'rgb(red)',
  '1+1', '2 × 3', '10 ÷ 4', '2^10', '2^3^2', '50%', '200*15%', '(1+2)*3', '-5+3', '+4', '1,000 + 2,500', '1/3', '2/3', '10/0', '0/0',
  '0.1+0.2', '1e5+1', '3.14159*2', '7 - 10', '((2))', '(1+2', '1+2)', '5 %', '1..2+3', '12-10-2026', '29/10/2026', '2026.10.29', '010-1234-5678',
  '+20-100-123-4567', '1-2', '3 - 2', '100', '1 2', '٣+٤', '1+1 ', '﻿1+1', ' 1+1', '1 2+3', '0.001953125*1', '0.0000000125+0',
  '123456789*987654321', '2^60', '2^0.5', '(-8)^(1/3)', '1.005*1', '1.255*1', '9.999999995+0', '-0.000000005+0',
  long(29), long(30), long(31), `${long(40)} `, 'x'.repeat(4001), `   ${long(30)}   `,
];

// generated arithmetic
let seed = 7;
const rnd = () => ((seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648);
const pick = (a) => a[Math.floor(rnd() * a.length)];
function num() {
  const r = rnd();
  if (r < 0.5) return String(Math.floor(rnd() * 1000));
  if (r < 0.8) return (rnd() * 100).toFixed(1 + Math.floor(rnd() * 4));
  return `${Math.floor(rnd() * 100)}%`;
}
function expr(depth) {
  if (depth > 2 || rnd() < 0.4) return num();
  const op = pick(['+', '-', '*', '/', '^', '×', '÷']);
  const a = expr(depth + 1);
  const b = expr(depth + 1);
  const sp = rnd() < 0.5 ? ' ' : '';
  const s = `${a}${sp}${op}${sp}${b}`;
  return rnd() < 0.3 ? `(${s})` : s;
}
const gen = Array.from({ length: 6000 }, () => expr(0));

const cases = [...hand, ...gen].map((text) => {
  try {
    return { text, out: classify(text) };
  } catch (e) {
    return { text, throws: e.message };
  }
});
fs.writeFileSync(path.join(out, 'smartclip.json'), JSON.stringify(cases));
console.log(`fixtures: smartclip ${cases.length} (${cases.filter((c) => c.throws).length} throw, ${cases.filter((c) => c.out && c.out.type === 'math').length} math)`);

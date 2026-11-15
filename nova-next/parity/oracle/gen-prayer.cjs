// References for nova-prayer, produced by the current prayer.js (calcDay) under Node's ICU:
//   prayer.json  23 places on every calculation method, ~2,300 days each from 1900 to 2076
//                (every 3rd day 2020-2031, every 31st day otherwise), Hanafi Asr on and off
//   hijri.json   the Hijri date of every day 1900-2076 (adjust 0), and every 5th day with -2..+2
//   node nova-next/parity/oracle/gen-prayer.cjs
require('./electron-stub.cjs');
const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '..', '..', '..');
const features = require(path.join(root, 'features.js'));
const { calcDay } = require(path.join(root, 'prayer.js'));
const out = path.join(__dirname, '..', 'fixtures');

const PLACES = [
  { city: 'Cairo', country: 'EG', lat: 30.0444, lon: 31.2357, timezone: 'Africa/Cairo' },
  { city: 'Mecca', country: 'SA', lat: 21.3891, lon: 39.8579, timezone: 'Asia/Riyadh' },
  { city: 'Dubai', country: 'AE', lat: 25.2048, lon: 55.2708, timezone: 'Asia/Dubai' },
  { city: 'Amman', country: 'JO', lat: 31.9454, lon: 35.9284, timezone: 'Asia/Amman' },
  { city: 'Tehran', country: 'IR', lat: 35.6892, lon: 51.389, timezone: 'Asia/Tehran' },
  { city: 'Istanbul', country: 'TR', lat: 41.0082, lon: 28.9784, timezone: 'Europe/Istanbul' },
  { city: 'Karachi', country: 'PK', lat: 24.8607, lon: 67.0011, timezone: 'Asia/Karachi' },
  { city: 'Jakarta', country: 'ID', lat: -6.2088, lon: 106.8456, timezone: 'Asia/Jakarta' },
  { city: 'Kuala Lumpur', country: 'MY', lat: 3.139, lon: 101.6869, timezone: 'Asia/Kuala_Lumpur' },
  { city: 'Singapore', country: 'SG', lat: 1.3521, lon: 103.8198, timezone: 'Asia/Singapore' },
  { city: 'Paris', country: 'FR', lat: 48.8566, lon: 2.3522, timezone: 'Europe/Paris' },
  { city: 'New York', country: 'US', lat: 40.7128, lon: -74.006, timezone: 'America/New_York' },
  { city: 'Moscow', country: 'RU', lat: 55.7558, lon: 37.6173, timezone: 'Europe/Moscow' },
  { city: 'Kuwait', country: 'KW', lat: 29.3759, lon: 47.9774, timezone: 'Asia/Kuwait' },
  { city: 'Doha', country: 'QA', lat: 25.2854, lon: 51.531, timezone: 'Asia/Qatar' },
  { city: 'Manama', country: 'BH', lat: 26.2285, lon: 50.586, timezone: 'Asia/Bahrain' },
  { city: 'Casablanca', country: 'MA', lat: 33.5731, lon: -7.5898, timezone: 'Africa/Casablanca' },
  { city: 'Algiers', country: 'DZ', lat: 36.7538, lon: 3.0588, timezone: 'Africa/Algiers' },
  { city: 'Tunis', country: 'TN', lat: 36.8065, lon: 10.1815, timezone: 'Africa/Tunis' },
  { city: 'Oslo', country: 'NO', lat: 59.9139, lon: 10.7522, timezone: 'Europe/Oslo' },
  { city: 'Tromso', country: 'NO', lat: 69.6492, lon: 18.9553, timezone: 'Europe/Oslo' },
  { city: 'Sydney', country: 'AU', lat: -33.8688, lon: 151.2093, timezone: 'Australia/Sydney' },
  // saved before countries were known: the method comes from the time zone
  { city: 'Old Cairo', country: '', lat: 30.0444, lon: 31.2357, timezone: 'Africa/Cairo' },
];

const DAY = 86400e3;
const ymd = (t) => { const d = new Date(t); return { y: d.getUTCFullYear(), m: d.getUTCMonth() + 1, d: d.getUTCDate() }; };
const days = [];
for (let t = Date.UTC(1900, 0, 1); t <= Date.UTC(2076, 11, 31); t += DAY) {
  const y = new Date(t).getUTCFullYear();
  const n = Math.round((t - Date.UTC(1900, 0, 1)) / DAY);
  if ((y >= 2020 && y <= 2031) ? n % 3 === 0 : n % 31 === 0) days.push(ymd(t));
}

const cases = [];
for (const [i, loc] of PLACES.entries()) {
  for (const [j, day] of days.entries()) {
    // Hanafi on every 4th day, and the Hijri adjustment cycling, so both paths are covered
    const hanafi = (i + j) % 4 === 0;
    const adjust = ((i + j) % 5) - 2;
    features.set({ asrHanafi: hanafi, hijriAdjust: adjust });
    // prayer.js throws when a time can't exist (no sunrise/sunset near the poles): record that too
    let res;
    try { res = { out: calcDay(day, loc, loc.timezone) }; } catch (e) { res = { error: e.message }; }
    cases.push({ loc, day, hanafi, adjust, ...res });
  }
}
fs.writeFileSync(path.join(out, 'prayer.json'), JSON.stringify(cases));

// Hijri on its own: every day, adjust 0; every 5th day with -2..+2
features.set({ asrHanafi: false, hijriAdjust: 0 });
const hijri = [];
const egypt = PLACES[0];
for (let t = Date.UTC(1900, 0, 1), n = 0; t <= Date.UTC(2076, 11, 31); t += DAY, n++) {
  const day = ymd(t);
  for (const adjust of n % 5 === 0 ? [-2, -1, 0, 1, 2] : [0]) {
    features.set({ hijriAdjust: adjust });
    const r = calcDay(day, egypt, egypt.timezone);
    hijri.push([day.y, day.m, day.d, adjust, r.hijri, r.hijriMonth]);
  }
}
fs.writeFileSync(path.join(out, 'hijri.json'), JSON.stringify(hijri));
console.log(`fixtures: prayer ${cases.length} days (${cases.filter((c) => c.error).length} throw), hijri ${hijri.length} dates, Node ICU ${process.versions.icu}, tz ${process.versions.tz}`);

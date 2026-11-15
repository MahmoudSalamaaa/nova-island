// References for the calendar, produced by the current calendar.js (node-ical 0.27 + rrule-temporal):
// generated .ics feeds the way Google, Outlook and iCloud write them (IANA and Windows TZIDs, UTC,
// floating times, all-day, RRULE with COUNT/UNTIL/INTERVAL/BYDAY/BYMONTHDAY/BYSETPOS, EXDATE,
// moved and cancelled occurrences, DURATION, folded lines, escaped text, join links), each read
// at moments around DST changes. Output: the events calendar.js would show for the next 24 h.
//   node nova-next/parity/oracle/gen-calendar.cjs
require('./electron-stub.cjs');
const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '..', '..', '..');
const ical = require(path.join(root, 'node_modules', 'node-ical', 'node-ical.cjs'));
const { occurrences } = require(path.join(root, 'calendar.js'));
const out = path.join(__dirname, '..', 'fixtures');

let seed = 11;
const rnd = () => ((seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648);
const pick = (a) => a[Math.floor(rnd() * a.length)];
const pad = (n) => String(n).padStart(2, '0');

const ZONES = [
  ['TZID=Africa/Cairo', ''], ['TZID=America/New_York', ''], ['TZID=Europe/London', ''], ['TZID=Asia/Riyadh', ''],
  ['TZID=Egypt Standard Time', ''], ['TZID="Pacific Standard Time"', ''], ['TZID=W. Europe Standard Time', ''],
  ['', 'Z'], ['', ''],
];
const RULES = [
  '', '', 'FREQ=DAILY;COUNT=10', 'FREQ=DAILY;INTERVAL=2', 'FREQ=WEEKLY;BYDAY=MO,WE,FR', 'FREQ=WEEKLY;INTERVAL=2;BYDAY=TU,TH;UNTIL=20270101T000000Z',
  'FREQ=MONTHLY;BYMONTHDAY=15', 'FREQ=MONTHLY;BYDAY=1MO', 'FREQ=MONTHLY;BYDAY=-1FR', 'FREQ=MONTHLY;BYDAY=MO,TU,WE,TH,FR;BYSETPOS=-1',
  'FREQ=YEARLY', 'FREQ=WEEKLY;COUNT=6;BYDAY=SU', 'FREQ=DAILY;UNTIL=20261110T235959Z',
];
const LINKS = ['https://meet.google.com/abc-defg-hij', 'https://us02web.zoom.us/j/1234567890?pwd=x).', 'https://teams.microsoft.com/l/meetup-join/19%3a1', 'https://acme.webex.com/meet/bob', ''];

function stamp(y, m, d, h, mi, z) {
  return `${y}${pad(m)}${pad(d)}T${pad(h)}${pad(mi)}00${z}`;
}
function fold(line) {
  if (line.length <= 75) return line;
  const parts = [line.slice(0, 75)];
  for (let i = 75; i < line.length; i += 74) parts.push(` ${line.slice(i, i + 74)}`);
  return parts.join('\r\n');
}

function feed(n) {
  const lines = ['BEGIN:VCALENDAR', 'VERSION:2.0', 'PRODID:-//Nova parity//EN'];
  for (let e = 0; e < 4 + Math.floor(rnd() * 5); e++) {
    const [tzp, z] = pick(ZONES);
    const y = 2026;
    const m = 9 + Math.floor(rnd() * 4);
    const d = 1 + Math.floor(rnd() * 28);
    const h = Math.floor(rnd() * 24);
    const mi = pick([0, 15, 30, 45]);
    const allDay = rnd() < 0.1;
    const rule = pick(RULES);
    const uid = `ev-${n}-${e}@nova`;
    const link = pick(LINKS);
    const param = tzp ? `;${tzp}` : '';
    lines.push('BEGIN:VEVENT', `UID:${uid}`, `DTSTAMP:20261001T000000Z`);
    if (allDay) lines.push(`DTSTART;VALUE=DATE:${y}${pad(m)}${pad(d)}`, `DTEND;VALUE=DATE:${y}${pad(m)}${pad(d + 1)}`);
    else {
      lines.push(`DTSTART${param}:${stamp(y, m, d, h, mi, z)}`);
      if (rnd() < 0.3) lines.push(`DURATION:PT${pick([15, 30, 45, 90])}M`);
      else lines.push(`DTEND${param}:${stamp(y, m, d, Math.min(23, h + 1), mi, z)}`);
    }
    if (rule) lines.push(`RRULE:${rule}`);
    if (rule && rnd() < 0.4) lines.push(`EXDATE${param}:${stamp(y, m, d + 2 > 28 ? d : d + 2, h, mi, z)}`);
    const kind = rnd();
    if (kind < 0.15) lines.push('SUMMARY:Stand-up\\, team A\\nroom 2');
    else if (kind < 0.25) lines.push(`SUMMARY;LANGUAGE=en:Review ${e}`);
    else if (kind > 0.95) lines.push('STATUS:CANCELLED', 'SUMMARY:Cancelled one');
    else if (kind > 0.9) { /* no summary */ } else lines.push(fold(`SUMMARY:Meeting ${n}-${e} ${'about the very long agenda '.repeat(Math.floor(rnd() * 4))}`));
    if (link) {
      const where = rnd();
      if (where < 0.3) lines.push(`LOCATION:${link}`);
      else if (where < 0.45) lines.push(`LOCATION;LANGUAGE=en:${link}`);
      else if (where < 0.7) lines.push(fold(`DESCRIPTION:Join here: ${link}\\nThanks`));
      else if (where < 0.85) lines.push(`X-GOOGLE-CONFERENCE:${link}`);
      else lines.push(`URL:${link}`);
    }
    lines.push('END:VEVENT');
    // a moved occurrence and a cancelled one for some series
    if (rule && !allDay && rnd() < 0.35) {
      lines.push('BEGIN:VEVENT', `UID:${uid}`, `RECURRENCE-ID${param}:${stamp(y, m, d + 1 > 28 ? d : d + 1, h, mi, z)}`,
        `DTSTART${param}:${stamp(y, m, d + 1 > 28 ? d : d + 1, Math.min(23, h + 2), mi, z)}`, `DTEND${param}:${stamp(y, m, d + 1 > 28 ? d : d + 1, Math.min(23, h + 3), mi, z)}`,
        'SUMMARY:Moved occurrence', 'END:VEVENT');
    }
    if (rule && !allDay && rnd() < 0.2) {
      lines.push('BEGIN:VEVENT', `UID:${uid}`, `RECURRENCE-ID${param}:${stamp(y, m, d + 3 > 28 ? d : d + 3, h, mi, z)}`,
        `DTSTART${param}:${stamp(y, m, d + 3 > 28 ? d : d + 3, h, mi, z)}`, 'STATUS:CANCELLED', 'END:VEVENT');
    }
  }
  lines.push('END:VCALENDAR');
  return `${lines.join('\r\n')}\r\n`;
}

// moments: around US/EU/Egypt DST changes in 2026 and spread through Sep-Dec
const NOW = [
  Date.UTC(2026, 9, 25, 0, 30), Date.UTC(2026, 9, 25, 12), Date.UTC(2026, 9, 29, 21), Date.UTC(2026, 10, 1, 5), Date.UTC(2026, 10, 1, 7),
  Date.UTC(2026, 9, 30, 20), Date.UTC(2026, 8, 15, 10), Date.UTC(2026, 9, 5, 22), Date.UTC(2026, 10, 20, 3), Date.UTC(2026, 11, 24, 18),
];

const cases = [];
for (let n = 0; n < 60; n++) {
  const ics = feed(n);
  const data = ical.sync.parseICS(ics);
  const results = NOW.map((now) => occurrences(data, new Date(now), new Date(now + 24 * 3600e3)));
  cases.push({ ics, now: NOW, results });
}
fs.writeFileSync(path.join(out, 'calendar.json'), JSON.stringify(cases));
const total = cases.reduce((s, c) => s + c.results.reduce((t, r) => t + r.length, 0), 0);
console.log(`fixtures: calendar ${cases.length} feeds x ${NOW.length} moments, ${total} events shown, PC zone ${Intl.DateTimeFormat().resolvedOptions().timeZone}`);

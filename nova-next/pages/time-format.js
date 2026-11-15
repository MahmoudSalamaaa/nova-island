// 12- or 24-hour times (1.8): Settings › General › Time format (the "clock24" feature). Every page
// that shows a time (the island, Settings, Tasks) loads this first; it makes each time a page
// writes with toLocaleTimeString / toLocaleString follow the setting, whatever the page asked for,
// so the island's clock, the prayer times, meetings and tasks all read the same way as the
// desktop widgets. A page tells it the setting with window.novaClock24(on).
(() => {
  let h24 = false;
  const fix = (opts, always) => {
    if (!always && !(opts && (opts.hour || opts.timeStyle))) return opts;
    const o = { ...(opts || {}) };
    delete o.hourCycle;
    o.hour12 = !h24;
    return o;
  };
  const time = Date.prototype.toLocaleTimeString;
  const both = Date.prototype.toLocaleString;
  // a formatter made once per language and options (making one is what costs: the island writes
  // a day's 1440 clock texts for Nova at a time, and a slow PC felt it)
  const cache = new Map();
  const formatter = (loc, o) => {
    const k = JSON.stringify([loc ?? null, o]);
    let f = cache.get(k);
    if (!f) {
      f = new Intl.DateTimeFormat(loc, o);
      if (cache.size > 64) cache.clear();
      cache.set(k, f);
    }
    return f;
  };
  // toLocaleTimeString's own rule: no time asked for, then hours, minutes and seconds
  const TIME = ['hour', 'minute', 'second', 'dayPeriod', 'fractionalSecondDigits', 'timeStyle'];
  const DATE = ['weekday', 'year', 'month', 'day', 'dateStyle'];
  Date.prototype.toLocaleTimeString = function (loc, opts) {
    // a time asked for in 24 hours on purpose is a narrow one (the vertical island's 44px): in 12
    // hours it stays as narrow, without AM / PM ("7:46")
    if (opts && opts.hour12 === false && !h24) {
      const o = { ...opts, hour: 'numeric', hour12: true };
      delete o.hourCycle;
      try {
        return formatter(loc, o).formatToParts(this).filter((p) => p.type !== 'dayPeriod').map((p) => p.value).join('').trim();
      } catch {}
    }
    const o = fix(opts, true);
    if (!Number.isNaN(this.getTime()) && !DATE.some((k) => o[k] !== undefined) && !(o.timeStyle && o.hour)) {
      const w = TIME.some((k) => o[k] !== undefined) ? o : { ...o, hour: 'numeric', minute: 'numeric', second: 'numeric' };
      try {
        return formatter(loc, w).format(this);
      } catch {}
    }
    return time.call(this, loc, o);
  };
  Date.prototype.toLocaleString = function (loc, opts) { return both.call(this, loc, fix(opts, false)); };
  window.novaClock24 = (on) => {
    const was = h24;
    h24 = !!on;
    return was !== h24;
  };
  // the island: its features come with every change; the clock is redrawn at once
  window.addEventListener('island:features', (e) => {
    if (window.novaClock24(e.detail?.clock24) && typeof window.tickClock === 'function') window.tickClock();
  });
  // Settings: the 12 / 24 choice in General (loaded before prefs.js, so its times follow it too)
  const api = window.prefs;
  const seg = document.querySelectorAll('[data-clock]');
  if (api && seg.length) {
    const show = (f) => {
      window.novaClock24(f?.clock24);
      seg.forEach((b) => b.setAttribute('aria-checked', String((b.dataset.clock === '24') === !!f?.clock24)));
    };
    seg.forEach((b) => b.addEventListener('click', () => {
      api.setFeatures({ clock24: b.dataset.clock === '24' });
      if (typeof window.saved === 'function') window.saved();
    }));
    api.onFeatures(show);
    api.features().then(show);
  }
})();

// Quran Radio (1.8's copy of views/radio.js): live Qur'an stations, and for a reciter his surahs
// in Mushaf order, played by the island itself so they keep going in the background. It comes with
// the app, on unless the user switched it off or removed it. It has its own Radio tab (station,
// play / stop, volume) and a button in the Prayer tab; stations and surahs are chosen in the Radio
// window (radio/index.html), not inside the island. The collapsed island stays as it is; a small
// radio with waves sits by the prayer countdown. A dropped stream reconnects by itself (a short
// stall is waited out first); a surah that ends goes on to the next one. "Pause at prayer time"
// stops it when the prayer comes in.
(() => {
  const RETRY_S = [2, 5, 15];
  const STALL_MS = 10000; // a live stream often pauses for a moment and goes on by itself
  const SURAH_KEY = 'nova.radio.surah'; // {station, n}: the surah to go back to
  const card = $('#radio');
  const prBtn = $('#pr-radio');
  const audio = new Audio();
  audio.preload = 'none';

  let plugin = null; // { id, stations, values }
  let station = null;
  let surah = null; // the surah number playing, or null for the live station
  let suwar = []; // surah names, 1-based through index - 1
  let want = false; // the user wants it playing
  let live = false;
  let fails = 0;
  let retryTimer = null;
  let stallTimer = null;

  bridge.radioSuwar().then((list) => { suwar = Array.isArray(list) ? list : []; sync(); }).catch(() => {});

  const surahName = (n) => (suwar[n - 1] ? `سورة ${suwar[n - 1]}` : `سورة ${n}`);
  const status = (t) => { $('#rd-status').textContent = t; };
  const showVol = (v) => { $('#rd-vol-pct').textContent = `${Math.round(v * 100)}%`; };
  const playingText = () => (surah ? surahName(surah) : station?.group === 'القرّاء' ? 'تلاوات متواصلة' : 'إذاعة رسمية');
  // the line above the name: the surah, live while the stream plays, the station's kind otherwise
  const eyebrow = () => {
    $('#rd-eyebrow').textContent = surah ? (station?.surahs?.rewaya || 'تلاوة') : want && live ? 'بث مباشر' : station?.group || 'راديو القرآن';
  };
  const saveSurah = () => {
    try { localStorage.setItem(SURAH_KEY, JSON.stringify(surah ? { station: station?.id, n: surah } : null)); } catch {}
  };
  const savedSurah = (id) => {
    try {
      const s = JSON.parse(localStorage.getItem(SURAH_KEY) || 'null');
      return s && s.station === id ? s.n : null;
    } catch { return null; }
  };
  // only a surah the station has
  const hasSurah = (st, n) => !!n && !!st?.surahs?.list?.includes(n);
  const url = () => (surah ? `${station.surahs.server}${String(surah).padStart(3, '0')}.mp3` : station.url);

  function sync() {
    state.radio = { on: want, name: station?.name || '' };
    card.classList.toggle('playing', want);
    card.classList.toggle('live', want && live);
    eyebrow();
    prBtn.classList.toggle('on', want);
    island.classList.toggle('radio-on', want);
    bridge.radioNow({ station: station?.id || null, surah, playing: want, live: want && live });
    render();
  }

  function stopAudio() {
    clearTimeout(retryTimer);
    clearTimeout(stallTimer);
    audio.pause();
    audio.removeAttribute('src');
    audio.load();
    live = false;
  }

  function connect() {
    if (!want || !station) return;
    clearTimeout(retryTimer);
    live = false;
    status(fails ? 'Reconnecting…' : 'جارٍ الاتصال…');
    audio.src = url();
    audio.volume = Math.min(1, Math.max(0, (plugin.values.volume ?? 80) / 100));
    audio.play().catch(() => {});
    sync();
  }

  function play() {
    if (!station) return;
    want = true;
    fails = 0;
    connect();
  }

  function stop(text = 'متوقفة') {
    want = false;
    stopAudio();
    status(text);
    sync();
  }

  // the stream dropped (no internet, the station restarted): try again, a little later each time
  function dropped() {
    clearTimeout(stallTimer);
    if (!want) return;
    live = false;
    const wait = RETRY_S[Math.min(fails, RETRY_S.length - 1)];
    fails++;
    status('Reconnecting…');
    sync();
    clearTimeout(retryTimer);
    retryTimer = setTimeout(connect, wait * 1000);
  }

  // a surah came to its end: the next one this reciter has, or the end of the Mushaf
  function nextSurah() {
    const list = station?.surahs?.list || [];
    const next = list.find((n) => n > surah);
    if (!next) {
      surah = null;
      saveSurah();
      stop('انتهت التلاوة');
      return;
    }
    surah = next;
    saveSurah();
    fails = 0;
    connect();
  }

  audio.addEventListener('playing', () => {
    clearTimeout(stallTimer);
    fails = 0;
    live = true;
    status(playingText());
    sync();
  });
  audio.addEventListener('ended', () => {
    if (!want || !audio.getAttribute('src')) return;
    if (surah) nextSurah();
    else dropped();
  });
  // an error: reconnect; a stall: only if it hasn't gone on in 10 s
  audio.addEventListener('error', () => {
    if (want && audio.getAttribute('src')) dropped();
  });
  ['waiting', 'stalled'].forEach((ev) => audio.addEventListener(ev, () => {
    if (!want || !audio.getAttribute('src')) return;
    if (live) status('جارٍ التحميل…');
    clearTimeout(stallTimer);
    stallTimer = setTimeout(() => { if (want && (audio.paused || audio.readyState < 3)) dropped(); }, STALL_MS);
  }));
  audio.addEventListener('timeupdate', () => {
    if (!stallTimer || audio.readyState < 3) return;
    clearTimeout(stallTimer);
    stallTimer = null;
    if (live) status(playingText());
  });
  // back online: no need to wait for the next try
  bridge.onNetwork((n) => {
    if (want && !live && n.internet) connect();
  });

  // ---------- controls ----------
  $('#rd-play').addEventListener('click', () => (want ? stop() : play()));
  prBtn.addEventListener('click', () => (want ? stop() : play()));
  // stations and surahs are chosen in the Radio window
  $('#rd-pick').addEventListener('click', () => bridge.radioOpen());

  const vol = makeSlider($('#rd-vol'), {
    onInput: (v) => {
      audio.volume = v;
      showVol(v);
    },
    onCommit: (v) => {
      if (!plugin) return;
      plugin.values.volume = Math.round(v * 100);
      bridge.pluginSet(plugin.id, 'volume', plugin.values.volume);
    },
  });

  // from the Radio window: a station live (n null) or one of a reciter's surahs, playing at once
  function choose(id, n) {
    const next = plugin?.stations.find((s) => s.id === id);
    if (!next) return;
    station = next;
    surah = hasSurah(next, n) ? n : null;
    $('#rd-name').textContent = station.name;
    saveSurah();
    if (plugin.values.station !== id) {
      plugin.values.station = id;
      bridge.pluginSet(plugin.id, 'station', id);
    }
    play();
  }
  bridge.onRadioPlay((p) => choose(p?.station, Number(p?.surah) || null));

  // the prayer came in: stop, if the user asked for it
  window.addEventListener('island:adhan', () => {
    if (want && plugin?.values.pauseAtPrayer) stop('متوقفة لوقت الصلاة');
  });

  // ---------- the plugin: installed, switched on/off, its settings ----------
  function apply(list) {
    const p = (list || []).find((x) => x.kind === 'radio') || null;
    plugin = p;
    card.hidden = !p;
    prBtn.hidden = !p;
    if (state.radioTab !== !!p) {
      state.radioTab = !!p;
      refreshTabs();
    }
    if (!p) {
      if (want) stop();
      station = null;
      surah = null;
      sync();
      return;
    }
    const next = p.stations.find((s) => s.id === p.values.station) || p.stations[0];
    const changed = next.id !== station?.id;
    station = next;
    // another station (Settings, or the first start): its saved surah, if any
    if (changed) surah = hasSurah(next, savedSurah(next.id)) ? savedSurah(next.id) : null;
    $('#rd-name').textContent = station.name;
    if (!vol.dragging) {
      vol.set((p.values.volume ?? 80) / 100);
      showVol((p.values.volume ?? 80) / 100);
    }
    audio.volume = (p.values.volume ?? 80) / 100;
    if (changed && want) connect();
    if (!want) status(surah ? surahName(surah) : 'اضغط للتشغيل');
    sync();
  }
  bridge.onPlugins(apply);
  bridge.plugins().then(apply);
})();

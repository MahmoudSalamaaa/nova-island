// (1.8.7) Internet use: ↓ ↑ and the ping in the System tab and on the closed island (the "Internet
// speed" slide), and "Internet is slow" when the ping stays high.
(() => {
  const rate = (bytes) => {
    if (bytes == null) return '—';
    // idle traffic is a few hundred bytes a second: one decimal under 10 KB/s, so it isn't "0"
    if (bytes < 50) return '0 KB/s';
    if (bytes < 1e4) return `${(bytes / 1000).toFixed(1)} KB/s`;
    if (bytes < 1e6) return `${Math.round(bytes / 1000)} KB/s`;
    return `${(bytes / 1e6).toFixed(bytes < 1e7 ? 1 : 0)} MB/s`;
  };

  // the toast: over 200 ms six readings in a row (about a minute at 10 s), once until it's back under 150
  let high = 0;
  let warned = false;
  function watch(ping) {
    if (ping == null) {
      high = 0;
      return;
    }
    if (ping > 200) high += 1;
    else if (ping < 150) {
      high = 0;
      warned = false;
    }
    if (high >= 6 && !warned && feat('slowNetToast')) {
      warned = true;
      showToast('net-off', 'Internet is slow', `${ping} ms`, 0, 4500);
      note('🐢', `Internet is slow (${ping} ms)`);
    }
  }

  // a new ping comes every 10 s; stats every 2 s carry the same one in between
  let lastPing;
  let pingSeen = 0;
  bridge.onStats((s) => {
    if (s.rx === undefined) return;
    const ping = s.ping ?? null;
    $('#i-rx').textContent = rate(s.rx);
    $('#i-tx').textContent = rate(s.tx);
    $('#i-ping').textContent = ping == null ? '' : `${ping} ms`;
    // for games: under 60 ms good (green), up to 120 fine (yellow), more is bad (red)
    const grade = ping == null ? '' : ping < 60 ? 'good' : ping <= 120 ? 'ok' : 'bad';
    for (const el of [$('#i-ping'), $('#net-ping')]) el.dataset.grade = grade;
    $('#net-live').textContent = `↓ ${rate(s.rx)}  ↑ ${rate(s.tx)}`;
    $('#net-ping').textContent = ping == null ? '' : `${ping} ms`;
    if (ping !== lastPing || Date.now() - pingSeen > 9500) {
      lastPing = ping;
      pingSeen = Date.now();
      watch(ping);
    }
  });

})();

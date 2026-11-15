// Settings › General › Check for updates (1.8.6): a check at once, and what it found, under
// Auto-update. A newer version is downloaded first; then the button becomes "Update now" (the
// island counts down, installs and reopens Nova), as the bar at the top of the page does. Hidden
// in the Store version, which the Store updates. Text is English; prefs-i18n.js adds the Arabic.
(() => {
  const row = document.getElementById('update-check-row');
  const btn = document.getElementById('upd-check');
  const msg = row?.querySelector('.upd-msg');
  const ver = row?.querySelector('.upd-ver');
  if (!row || !btn || !window.prefs?.checkUpdates) return;
  let current = '';
  let ready = null;

  const say = (text, version, tone) => {
    msg.textContent = text;
    ver.textContent = version || '';
    row.dataset.tone = tone || '';
  };
  const idle = () => {
    btn.disabled = false;
    btn.textContent = ready ? 'Update now' : 'Check now';
    btn.classList.toggle('accent', !!ready);
  };

  btn.addEventListener('click', () => {
    if (ready) {
      btn.disabled = true;
      btn.textContent = 'Updating…';
      window.prefs.updateNow();
      return;
    }
    btn.disabled = true;
    btn.textContent = 'Checking…';
    say('Checking…', '', '');
    window.prefs.checkUpdates();
  });

  window.prefs.onUpdateCheck?.((r) => {
    if (r?.state === 'ready') {
      ready = r.version;
      say('A new version is ready', r.version, 'new');
    } else if (r?.state === 'latest') {
      say('You have the latest version', current || r.version, 'ok');
    } else {
      say("Couldn't check. Check your internet connection and try again", '', 'bad');
    }
    idle();
  });
  // an update found by the scheduled check
  window.prefs.onUpdate?.((u) => {
    if (u?.version) {
      ready = u.version;
      say('A new version is ready', u.version, 'new');
      idle();
    }
  });
  const general = (g) => {
    if (!g) return;
    current = g.version || current;
    row.hidden = !!g.store;
    if (!ready && !row.dataset.tone) say('Version', current, '');
  };
  window.prefs.general().then(general);
  window.prefs.onGeneral?.(general);
  window.prefs.updateState?.().then((s) => {
    if (s?.ready) {
      ready = s.ready;
      say('A new version is ready', s.ready, 'new');
      idle();
    }
  });
})();

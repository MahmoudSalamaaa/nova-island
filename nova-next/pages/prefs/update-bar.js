// Settings (1.8): while a downloaded update waits, a bar at the top of every page says so, with
// "Update now" (the island counts down, installs and reopens Nova). Text is English here;
// prefs-i18n.js puts the Arabic over it.
(() => {
  const content = document.getElementById('content');
  if (!content || !window.prefs || !window.prefs.updateState) return;

  const bar = document.createElement('div');
  bar.className = 'update-bar';
  bar.hidden = true;
  const dot = document.createElement('i');
  const txt = document.createElement('div');
  txt.className = 'txt';
  const title = document.createElement('b');
  const version = document.createElement('span');
  version.className = 'ver';
  version.setAttribute('data-no-i18n', '');
  const sub = document.createElement('small');
  sub.textContent = 'A new version of Nova Island is ready to install';
  const head = document.createElement('div');
  head.append(title, version);
  title.textContent = 'Update ready';
  txt.append(head, sub);
  const btn = document.createElement('button');
  btn.className = 'btn accent';
  btn.textContent = 'Update now';
  btn.addEventListener('click', () => {
    btn.disabled = true;
    btn.textContent = 'Updating…';
    window.prefs.updateNow();
  });
  bar.append(dot, txt, btn);
  content.prepend(bar);

  const show = (s) => {
    const v = s && (s.ready || s.version);
    // (1.8.7) the installer didn't start: the button works again
    if (s && s.failed) {
      btn.disabled = false;
      btn.textContent = 'Update now';
    }
    bar.hidden = !v;
    if (v) version.textContent = v;
  };
  window.prefs.updateState().then(show);
  window.prefs.onUpdate?.(show);
})();

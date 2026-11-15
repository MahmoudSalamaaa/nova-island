// (1.8.7) Settings › Prayer & Dhikr › the adhan's files (the Fajr one and the one for the other
// prayers): play it, choose another, back to Nova's own.
(() => {
  const api = window.prefs;
  if (!api.adhan) return;
  const audio = new Audio();
  let playing = null; // the row whose adhan plays

  function stop() {
    audio.pause();
    if (playing) playing.querySelector('.adhan-play').textContent = 'Play';
    playing = null;
  }
  audio.addEventListener('ended', stop);
  audio.addEventListener('error', stop);

  function show(info) {
    if (!info) return;
    for (const row of document.querySelectorAll('.adhan-row')) {
      const which = row.dataset.adhan;
      const i = info[which];
      if (!i) continue;
      // Fajr with nothing of its own plays the other adhan
      const usesMain = which === 'fajr' && !i.custom && !i.hasDefault;
      const main = info.main;
      row.querySelector('.adhan-name').textContent = i.custom ? i.name
        : i.hasDefault ? (which === 'fajr' ? "Nova's Fajr adhan" : "Nova's adhan")
          : usesMain && (main.custom || main.hasDefault) ? 'Same as the adhan' : 'No file yet';
      row.querySelector('.adhan-reset').hidden = !i.custom;
      row.querySelector('.adhan-play').disabled = !(i.custom || i.hasDefault || main.custom || main.hasDefault);
    }
  }
  const refresh = async () => show(await api.adhan());

  for (const row of document.querySelectorAll('.adhan-row')) {
    const which = row.dataset.adhan;
    const play = row.querySelector('.adhan-play');
    play.addEventListener('click', () => {
      const was = playing;
      stop();
      if (was === row) return;
      audio.src = `/adhan${which === 'fajr' ? '-fajr' : ''}-audio?v=${Date.now()}`;
      audio.play().then(() => { playing = row; play.textContent = 'Stop'; }).catch(stop);
    });
    row.querySelector('.adhan-pick').addEventListener('click', async () => {
      stop();
      const r = await api.adhanPick(which);
      if (r && r.error) {
        row.querySelector('.adhan-name').textContent = r.error === 'size' ? 'The file is too large (30 MB at most)'
          : r.error === 'type' ? 'Choose an audio file (mp3, m4a, ogg, wav)' : "Couldn't use the file";
      } else refresh();
    });
    row.querySelector('.adhan-reset').addEventListener('click', async () => {
      stop();
      await api.adhanReset(which);
      refresh();
    });
  }
  refresh();
})();

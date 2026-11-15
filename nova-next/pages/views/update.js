// A new version is downloaded (1.8, in place of 1.7's): it can't be missed now.
//   the card    "Update ready · 1.8.x · Update now" stays a minute, and comes back every 15
//               minutes until the update is installed (1.7 showed it 20 s, again every 6 hours)
//   the dot     a small green dot on the island the whole time an update is waiting
//   Settings    a bar at the top of every page with "Update now" (pages/prefs/update-bar.js);
//               its button asks the island to do what its own card does
// "Update now" first says Nova is about to restart, counting down 10 seconds, then installs and
// reopens. It also installs when the app quits.
(() => {
  const COUNTDOWN_S = 10;
  const SHOW_MS = 60_000;
  const AGAIN_MS = 15 * 60_000;
  let timer = null;
  let ready = null;
  let again = null;
  let installing = false;

  function countdown() {
    if (timer) return;
    let left = COUNTDOWN_S;
    // stays put under the cursor; a click just shows it again
    const show = () => {
      showToast('updating', 'Updating · back in a few seconds', String(left), 0, left * 1000 + 600);
      state.toastAction = show;
    };
    show();
    timer = setInterval(() => {
      left -= 1;
      if (left > 0) {
        $('#t-value').textContent = String(left);
        $('#vt-value').textContent = String(left);
        return;
      }
      clearInterval(timer);
      bridge.installUpdate();
      // (1.8.7) the countdown is over: an installer that didn't start ("update" with failed)
      // can be tried again
      timer = 0;
      installing = true;
    }, 1000);
  }

  function offer() {
    if (timer || !ready) return;
    showToast('update', `Update ready · ${ready}`, 'Update now', 0, SHOW_MS);
    state.toastAction = countdown;
  }

  bridge.onUpdate((u) => {
    // (1.8.7) the installer didn't start (blocked, or the file was gone): say so, and offer it
    // again when there's still one to install
    if (u && u.failed) {
      clearInterval(timer);
      timer = null;
      installing = false;
      if (!u.version) {
        ready = null;
        island.classList.remove('update-ready');
        clearInterval(again);
        again = null;
      }
      showToast('update', "Update didn't start", u.version ? 'Try again' : '', 0, SHOW_MS);
      state.toastAction = u.version ? countdown : null;
      return;
    }
    if (timer || installing || !u || !u.version) return;
    const first = ready == null;
    ready = u.version;
    island.classList.add('update-ready');
    offer();
    if (first || !again) {
      clearInterval(again);
      again = setInterval(offer, AGAIN_MS);
    }
  });
  // Settings' "Update now"
  bridge.onUpdateNow?.(() => {
    if (ready) countdown();
  });
})();

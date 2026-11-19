// Focus Timer panel. Opt-in module: call window.mountFocusTimer(root, options)
// from the island's plugin-aware renderer when the signed focus-timer kind is active.
// Does not access the network or run arbitrary plugin code.
(() => {
  const clamp = (value, fallback, min, max) => {
    const n = Number(value);
    return Number.isFinite(n) ? Math.min(max, Math.max(min, Math.round(n))) : fallback;
  };

  function mount(root, options = {}) {
    if (!(root instanceof Element)) throw new TypeError('Focus Timer requires a root element');
    const focusSeconds = clamp(options.focusMinutes, 25, 5, 120) * 60;
    const breakSeconds = clamp(options.breakMinutes, 5, 1, 30) * 60;
    let phase = 'focus', remaining = focusSeconds, deadline = null, ticker = null;
    let status = 'idle';
    const panel = document.createElement('section');
    panel.className = 'focus-timer-panel';
    panel.setAttribute('aria-label', 'Focus timer');
    const label = document.createElement('strong');
    const time = document.createElement('output');
    time.setAttribute('role', 'timer');
    time.setAttribute('aria-live', 'off');
    const controls = document.createElement('div');
    const make = (title, action) => {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = title;
      button.addEventListener('click', action);
      controls.append(button);
      return button;
    };
    const secondsLeft = () => deadline === null ? remaining : Math.max(0, Math.ceil((deadline - performance.now()) / 1000));
    const stopTicking = () => { if (ticker !== null) clearInterval(ticker); ticker = null; };
    const paint = () => {
      const left = secondsLeft();
      label.textContent = phase === 'focus' ? 'Focus' : 'Break';
      time.textContent = `${String(Math.floor(left / 60)).padStart(2, '0')}:${String(left % 60).padStart(2, '0')}`;
      toggle.textContent = status === 'running' ? 'Pause' : status === 'paused' ? 'Resume' : 'Start';
      toggle.disabled = status === 'completed';
    };
    const tick = () => {
      if (status !== 'running') return;
      if (secondsLeft() === 0) {
        remaining = 0;
        deadline = null;
        status = 'completed';
        stopTicking();
        paint();
        options.onComplete?.(phase);
      } else paint();
    };
    const toggle = make('Start', () => {
      if (status === 'running') {
        remaining = secondsLeft();
        deadline = null;
        status = remaining === 0 ? 'completed' : 'paused';
        stopTicking();
      } else if (status === 'idle' || status === 'paused') {
        deadline = performance.now() + remaining * 1000;
        status = 'running';
        ticker = setInterval(tick, 250);
      }
      paint();
    });
    make('Reset', () => {
      stopTicking();
      deadline = null;
      remaining = phase === 'focus' ? focusSeconds : breakSeconds;
      status = 'idle';
      paint();
    });
    const switchButton = make('Switch to break', () => {
      stopTicking();
      phase = phase === 'focus' ? 'break' : 'focus';
      deadline = null;
      remaining = phase === 'focus' ? focusSeconds : breakSeconds;
      status = 'idle';
      switchButton.textContent = phase === 'focus' ? 'Switch to break' : 'Switch to focus';
      paint();
    });
    panel.append(label, time, controls);
    root.replaceChildren(panel);
    paint();
    return {
      destroy() { stopTicking(); panel.remove(); },
      getState() { return { phase, status, remainingSeconds: secondsLeft() }; }
    };
  }
  window.mountFocusTimer = mount;
})();

// The signed plugin list controls visibility and lifecycle; never mount for an unknown kind.
(() => {
  const panel = document.getElementById('v-focus-timer');
  const mountPoint = document.getElementById('focus-timer-mount');
  if (!panel || !mountPoint || !window.nova?.plugins || !window.nova?.onPlugins) return;
  let instance = null;
  let config = '';
  function apply(list) {
    const plugin = Array.isArray(list) ? list.find(p => p.kind === 'focus-timer') : null;
    if (!plugin) {
      instance?.destroy();
      instance = null;
      config = '';
      state.focusTimerTab = false;
      refreshTabs();
      return;
    }
    const values = plugin.values || {};
    const next = JSON.stringify([values.focusMinutes ?? 25, values.breakMinutes ?? 5]);
    if (instance && config === next) return;
    instance?.destroy();
    config = next;
    instance = window.mountFocusTimer(mountPoint, {
      focusMinutes: values.focusMinutes,
      breakMinutes: values.breakMinutes,
      onComplete: (phase) => {
        if (typeof window.showToast === 'function')
          window.showToast('done', phase === 'focus' ? 'Focus session complete' : 'Break complete', '✓', 0, 4000);
      }
    });
    if (!state.focusTimerTab) {
      state.focusTimerTab = true;
      refreshTabs();
    }
  }
  window.nova.onPlugins(apply);
  window.nova.plugins().then(apply).catch(() => {});
})();

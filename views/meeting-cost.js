// Meeting Cost Live: estimated local-only running cost; no external requests.
(() => {
  const root = document.getElementById('meeting-cost-mount');
  if (!root || !window.nova?.onPlugins || !window.nova?.plugins) return;
  let active = false, running = false, elapsedMs = 0, startedAt = 0, interval = null;
  let attendees = 5, rate = 250, currency = 'EGP';
  const section = document.createElement('section');
  section.className = 'meeting-cost-card';
  const heading = document.createElement('strong');
  heading.textContent = 'Meeting Cost Live';
  const amount = document.createElement('output');
  amount.setAttribute('aria-label', 'Estimated meeting cost');
  const clock = document.createElement('span');
  const controls = document.createElement('div');
  const make = (label, fn) => {
    const b = document.createElement('button');
    b.type = 'button';
    b.textContent = label;
    b.addEventListener('click', fn);
    controls.append(b);
    return b;
  };
  const totalTime = () => elapsedMs + (running ? Math.max(0, performance.now() - startedAt) : 0);
  const paint = () => {
    const ms = totalTime();
    const seconds = Math.floor(ms / 1000);
    amount.textContent = new Intl.NumberFormat('en', { maximumFractionDigits: 2, minimumFractionDigits: 2 }).format((ms / 3600000) * attendees * rate) + ' ' + currency;
    clock.textContent = `${String(Math.floor(seconds / 3600)).padStart(2, '0')}:${String(Math.floor(seconds / 60) % 60).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`;
    toggle.textContent = running ? 'Pause' : ms ? 'Resume' : 'Start meeting';
  };
  const toggle = make('Start meeting', () => {
    if (running) { elapsedMs = totalTime(); running = false; }
    else { startedAt = performance.now(); running = true; }
    paint();
  });
  make('Reset', () => { running = false; elapsedMs = 0; startedAt = 0; paint(); });
  const fields = document.createElement('div');
  fields.className = 'meeting-cost-fields';
  const field = (label, min, max, value, change) => {
    const wrap = document.createElement('label');
    wrap.textContent = label;
    const input = document.createElement('input');
    input.type = 'number'; input.min = String(min); input.max = String(max); input.step = '1'; input.value = String(value);
    input.addEventListener('change', () => {
      const n = Number(input.value);
      if (!Number.isFinite(n)) { input.value = String(value); return; }
      change(Math.min(max, Math.max(min, Math.round(n))));
      paint();
    });
    wrap.append(input); fields.append(wrap);
  };
  field('Participants', 1, 100, attendees, v => { attendees = v; });
  field('Hourly rate per person', 0, 10000, rate, v => { rate = v; });
  const note = document.createElement('small');
  note.textContent = 'Estimate only · Local calculation · No employee data collected';
  section.append(heading, amount, clock, fields, controls, note);
  root.append(section);
  interval = setInterval(() => { if (active && running) paint(); }, 250);
  function apply(list) {
    const plugin = Array.isArray(list) ? list.find(p => p.kind === 'meeting-cost') : null;
    active = !!plugin;
    if (!active && running) { elapsedMs = totalTime(); running = false; }
    if (active && plugin?.values && elapsedMs === 0 && !running) {
      attendees = Math.max(1, Math.min(100, Number(plugin.values.attendees) || 5));
      rate = Math.max(0, Math.min(10000, Number(plugin.values.hourlyRate) || 0));
    }
    state.meetingCostTab = active;
    refreshTabs();
    paint();
  }
  window.nova.onPlugins(apply);
  window.nova.plugins().then(apply).catch(() => {});
  paint();
})();

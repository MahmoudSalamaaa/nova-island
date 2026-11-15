// The Tasks window (1.8.3): every task, by view (Today, Upcoming, All, Done) and tag, grouped by
// when it's due; a quick add on top and an editor on the side. The app keeps the list and checks
// every change (nova-services/tasks.rs). English or Arabic, as Settings › General › Language.
(() => {
  const api = window.tasks;
  const $ = (s, el = document) => el.querySelector(s);
  const $$ = (s, el = document) => [...el.querySelectorAll(s)];
  const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

  const AR = {
    Tasks: 'المهام', 'Search tasks': 'ابحث في المهام', Today: 'اليوم', Upcoming: 'القادمة', 'All tasks': 'كل المهام', Done: 'المنجزة',
    Tags: 'الوسوم', 'By time': 'حسب الوقت', 'By priority': 'حسب الأولوية', 'Clear done': 'مسح المنجزة', 'Add a task, then press Enter': 'أضف مهمة ثم اضغط Enter',
    Add: 'إضافة', 'No date': 'بلا موعد', Tomorrow: 'غدًا', 'Pick…': 'اختر…', Task: 'المهمة', Title: 'العنوان', Notes: 'ملاحظات', Due: 'الموعد',
    Reminder: 'التذكير', Tag: 'الوسم', Delete: 'حذف', Save: 'حفظ', Overdue: 'متأخرة', Later: 'لاحقًا', 'This week': 'هذا الأسبوع',
    low: 'منخفضة', medium: 'متوسطة', high: 'عالية', urgent: 'عاجلة', 'No reminder': 'بلا تذكير', 'At the time': 'في الموعد',
    'Nothing for today': 'لا شيء لليوم', 'Nothing coming up': 'لا شيء قادم', 'No tasks yet': 'لا توجد مهام بعد', 'Nothing done yet': 'لم تُنجز أي مهمة بعد',
    'No matching tasks': 'لا توجد مهام مطابقة', 'Add one above, or enjoy the free time.': 'أضف واحدة في الأعلى، أو استمتع بوقت الفراغ.',
    'Task added': 'أُضيفت المهمة', 'Task deleted': 'حُذفت المهمة', Saved: 'تم الحفظ', 'Edit': 'تعديل', Undo: 'تراجع',
    'Created': 'أُنشئت', 'Finished': 'أُنجزت', 'open': 'مفتوحة', 'done': 'منجزة', Yesterday: 'أمس',
  };
  let lang = 'en';
  const t = (s) => (lang === 'ar' ? AR[s] ?? s : s);
  // a tag's name: Low, Medium, High, Urgent (Arabic from AR)
  const tagName = (g) => (lang === 'ar' ? AR[g] ?? g : g.charAt(0).toUpperCase() + g.slice(1));
  const locale = () => (lang === 'ar' ? 'ar-EG-u-nu-latn' : 'en-US');

  const TAG_ORDER = { urgent: 0, high: 1, medium: 2, low: 3 };
  let tags = ['low', 'medium', 'high', 'urgent'];
  let reminders = [0, 5, 10, 15, 30, 60, 120, 1440];
  let list = [];
  let view = 'today';
  let tagFilter = null;
  let sort = 'due';
  let query = '';
  let editing = null;
  let addTag = 'medium';
  let addWhen = 'none';

  // ---- time ----
  const DAY = 86400000;
  const startOfDay = (d = new Date()) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const toLocalInput = (ms) => {
    if (!ms) return '';
    const d = new Date(ms);
    const p = (n) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
  };
  const fromLocalInput = (s) => (s ? new Date(s).getTime() || null : null);
  const clock = (ms) => new Date(ms).toLocaleTimeString(locale(), { hour: 'numeric', minute: '2-digit' });
  function whenText(ms) {
    const day = startOfDay(new Date(ms));
    const today = startOfDay();
    if (day === today) return `${t('Today')} · ${clock(ms)}`;
    if (day === today + DAY) return `${t('Tomorrow')} · ${clock(ms)}`;
    if (day === today - DAY) return `${t('Yesterday')} · ${clock(ms)}`;
    const sameYear = new Date(ms).getFullYear() === new Date().getFullYear();
    return `${new Date(ms).toLocaleDateString(locale(), { weekday: 'short', day: 'numeric', month: 'short', ...(sameYear ? {} : { year: 'numeric' }) })} · ${clock(ms)}`;
  }
  function remindText(m) {
    if (m == null) return t('No reminder');
    if (m === 0) return t('At the time');
    if (lang === 'ar') {
      const AR_BEFORE = { 5: 'قبلها بخمس دقائق', 10: 'قبلها بعشر دقائق', 15: 'قبلها بربع ساعة', 30: 'قبلها بنصف ساعة', 60: 'قبلها بساعة', 120: 'قبلها بساعتين', 1440: 'قبلها بيوم' };
      return AR_BEFORE[m] || `قبلها بـ ${m} دقيقة`;
    }
    if (m === 1440) return '1 day before';
    if (m >= 60) return `${m / 60} h before`;
    return `${m} min before`;
  }

  // ---- what each view shows ----
  const overdue = (x) => !x.done && x.due && x.due < Date.now();
  const inView = (x) => {
    const today = startOfDay();
    if (view === 'done') return x.done;
    if (x.done) return false;
    if (view === 'today') return x.due != null && x.due < today + DAY;
    if (view === 'upcoming') return x.due != null && x.due >= today + DAY;
    return true;
  };
  const matches = (x) => (!tagFilter || x.tag === tagFilter) && (!query || `${x.title} ${x.notes}`.toLowerCase().includes(query));
  function groupOf(x) {
    if (view === 'done') return null;
    if (x.due == null) return 'No date';
    const today = startOfDay();
    if (x.due < Date.now() && x.due < today + DAY && startOfDay(new Date(x.due)) < today) return 'Overdue';
    if (x.due < today + DAY) return 'Today';
    if (x.due < today + 2 * DAY) return 'Tomorrow';
    if (x.due < today + 7 * DAY) return 'This week';
    return 'Later';
  }
  const GROUPS = ['Overdue', 'Today', 'Tomorrow', 'This week', 'Later', 'No date'];
  const byDue = (a, b) => (a.due ?? Infinity) - (b.due ?? Infinity) || TAG_ORDER[a.tag] - TAG_ORDER[b.tag] || b.created - a.created;
  const byTag = (a, b) => TAG_ORDER[a.tag] - TAG_ORDER[b.tag] || (a.due ?? Infinity) - (b.due ?? Infinity) || b.created - a.created;

  // ---- drawing ----
  const ICON_CHECK = '<svg viewBox="0 0 24 24"><path d="m6 12.5 4 4 8-9"/></svg>';
  const ICON_CLOCK = '<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></svg>';
  const ICON_BELL = '<svg viewBox="0 0 24 24"><path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9"/><path d="M10.3 21a1.94 1.94 0 0 0 3.4 0"/></svg>';
  const ICON_NOTE = '<svg viewBox="0 0 24 24"><path d="M4 6h16M4 12h16M4 18h10"/></svg>';
  const ICON_EMPTY = '<svg viewBox="0 0 24 24"><rect x="4" y="4" width="16" height="16" rx="4"/><path d="m8.5 12 2.5 2.5 4.5-5"/></svg>';

  function taskRow(x) {
    const el = document.createElement('div');
    el.className = `task tag-${x.tag}${x.done ? ' done' : ''}${overdue(x) ? ' late' : ''}${editing === x.id ? ' sel' : ''}`;
    el.dataset.id = x.id;
    const meta = [];
    if (x.due) meta.push(`<span class="m-due">${ICON_CLOCK}${esc(whenText(x.due))}</span>`);
    if (x.due && x.remind != null && !x.done) meta.push(`<span class="m-rem" title="${esc(remindText(x.remind))}">${ICON_BELL}</span>`);
    if (x.notes) meta.push(`<span class="m-note">${ICON_NOTE}${esc(x.notes.split('\n')[0].slice(0, 80))}</span>`);
    el.innerHTML =
      `<button class="check" role="checkbox" aria-checked="${x.done}" aria-label="${esc(t('Done'))}">${ICON_CHECK}</button>` +
      `<div class="t-main"><div class="t-title" dir="auto"><span class="t-txt">${esc(x.title)}</span></div>${meta.length ? `<div class="t-meta">${meta.join('')}</div>` : ''}</div>` +
      `<span class="pill">${esc(tagName(x.tag))}</span>`;
    $('.check', el).addEventListener('click', (e) => {
      e.stopPropagation();
      if (el.classList.contains('finishing')) return;
      // (1.8.4) finishing a task: the title is struck through, then the row slides away and
      // closes up; it's saved as done once it has gone (a task brought back is saved at once)
      if (!x.done && view !== 'done') {
        el.classList.add('finishing');
        setTimeout(() => api.update(x.id, { done: true }), 720);
      } else {
        el.classList.toggle('done');
        api.update(x.id, { done: !x.done });
      }
    });
    el.addEventListener('click', () => openEditor(x.id));
    return el;
  }

  function render() {
    const counts = {
      today: list.filter((x) => !x.done && x.due != null && x.due < startOfDay() + DAY).length,
      upcoming: list.filter((x) => !x.done && x.due != null && x.due >= startOfDay() + DAY).length,
      all: list.filter((x) => !x.done).length,
      done: list.filter((x) => x.done).length,
    };
    for (const a of $$('[data-view]')) {
      a.classList.toggle('on', a.dataset.view === view);
      $('.n', a).textContent = counts[a.dataset.view] || '';
      $('.n', a).classList.toggle('late', a.dataset.view === 'today' && list.some(overdue));
    }
    const tagBox = $('#tag-filters');
    tagBox.innerHTML = tags.map((g) => `<a href="#" data-tagf="${g}" class="tagf tag-${g}${tagFilter === g ? ' on' : ''}"><i></i><span>${esc(tagName(g))}</span><span class="n">${list.filter((x) => !x.done && x.tag === g).length || ''}</span></a>`).join('');

    const titles = { today: 'Today', upcoming: 'Upcoming', all: 'All tasks', done: 'Done' };
    $('#view-title').textContent = t(titles[view]) + (tagFilter ? ` · ${tagName(tagFilter)}` : '');
    const shown = list.filter(inView).filter(matches).sort(sort === 'tag' ? byTag : byDue);
    if (view === 'done') shown.sort((a, b) => (b.doneAt || 0) - (a.doneAt || 0));
    const n = shown.length;
    $('#view-sub').textContent = lang === 'ar' ? (n ? `${n} ${view === 'done' ? t('done') : t('open')}` : '') : (n ? `${n} ${view === 'done' ? 'done' : n === 1 ? 'task' : 'tasks'}` : '');
    $('#clear-done').hidden = view !== 'done' || !n;
    $('#sort').hidden = view === 'done';
    for (const b of $$('#sort button')) b.setAttribute('aria-checked', String(b.dataset.sort === sort));
    $('#adder').hidden = view === 'done';

    const box = $('#list');
    box.innerHTML = '';
    if (sort === 'due' && view !== 'done') {
      for (const g of GROUPS) {
        const items = shown.filter((x) => groupOf(x) === g);
        if (!items.length) continue;
        const h = document.createElement('div');
        h.className = `group-h${g === 'Overdue' ? ' late' : ''}`;
        h.innerHTML = `<span>${esc(tagName(g))}</span><span class="c">${items.length}</span>`;
        box.append(h, ...items.map(taskRow));
      }
    } else {
      box.append(...shown.map(taskRow));
    }
    const empty = $('#empty');
    empty.hidden = n > 0;
    if (!n) {
      const msg = query || tagFilter ? 'No matching tasks' : { today: 'Nothing for today', upcoming: 'Nothing coming up', all: 'No tasks yet', done: 'Nothing done yet' }[view];
      empty.innerHTML = `${ICON_EMPTY}<b>${esc(t(msg))}</b>${view !== 'done' && !query ? `<span>${esc(t('Add one above, or enjoy the free time.'))}</span>` : ''}`;
    }
    if (editing && !list.some((x) => x.id === editing)) closeEditor();
  }

  // ---- the quick add ----
  function tagButtons(box, current, onPick) {
    box.innerHTML = tags.map((g) => `<button type="button" role="radio" class="tagb tag-${g}" data-tag="${g}" aria-checked="${g === current}"><i></i>${esc(tagName(g))}</button>`).join('');
    for (const b of $$('button', box)) b.addEventListener('click', () => { onPick(b.dataset.tag); for (const o of $$('button', box)) o.setAttribute('aria-checked', String(o === b)); });
  }
  function reminderOptions(sel, value) {
    sel.innerHTML = [null, ...reminders].map((m) => `<option value="${m ?? ''}"${(m ?? '') === (value ?? '') ? ' selected' : ''}>${esc(remindText(m))}</option>`).join('');
  }
  function addDue() {
    const now = new Date();
    if (addWhen === 'today') {
      const d = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 18, 0);
      return d.getTime() > Date.now() ? d.getTime() : Date.now() + 3600000;
    }
    if (addWhen === 'tomorrow') return new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 9, 0).getTime();
    if (addWhen === 'pick') return fromLocalInput($('#add-due').value);
    return null;
  }
  function setWhen(w) {
    addWhen = w;
    for (const b of $$('#add-when button')) b.classList.toggle('on', b.dataset.when === w);
    $('#add-due').hidden = w !== 'pick';
    if (w === 'pick' && !$('#add-due').value) $('#add-due').value = toLocalInput(Date.now() + 3600000);
    $('#add-remind').disabled = w === 'none';
  }
  $$('#add-when button').forEach((b) => b.addEventListener('click', () => setWhen(b.dataset.when)));
  $('#adder').addEventListener('submit', async (e) => {
    e.preventDefault();
    const title = $('#add-title').value.trim();
    if (!title) return;
    const due = addDue();
    const r = $('#add-remind').value;
    const saved = await api.add({ title, tag: addTag, due, remind: due != null && r !== '' ? Number(r) : null });
    if (saved) {
      $('#add-title').value = '';
      toast(t('Task added'));
      // a dated task in a view that wouldn't show it: show where it went
      if (view === 'today' && due != null && due >= startOfDay() + DAY) view = 'upcoming';
      if (view === 'upcoming' && (due == null || due < startOfDay() + DAY)) view = due == null ? 'all' : 'today';
    }
  });

  // ---- the editor ----
  let edTag = 'medium';
  function openEditor(id) {
    const x = list.find((y) => y.id === id);
    if (!x) return;
    editing = id;
    edTag = x.tag;
    $('#ed-title').value = x.title;
    $('#ed-notes').value = x.notes || '';
    $('#ed-due').value = toLocalInput(x.due);
    reminderOptions($('#ed-remind'), x.remind);
    $('#ed-remind').disabled = !x.due;
    tagButtons($('#ed-tag'), x.tag, (g) => { edTag = g; });
    const fmt = (ms) => new Date(ms).toLocaleString(locale(), { dateStyle: 'medium', timeStyle: 'short' });
    $('#ed-meta').textContent = `${t('Created')} ${fmt(x.created)}${x.done && x.doneAt ? ` · ${t('Finished')} ${fmt(x.doneAt)}` : ''}`;
    $('#editor').hidden = false;
    document.body.classList.add('editing');
    render();
    $('#ed-title').focus();
  }
  function closeEditor() {
    editing = null;
    $('#editor').hidden = true;
    document.body.classList.remove('editing');
    render();
  }
  $('#ed-close').addEventListener('click', closeEditor);
  $('#ed-due').addEventListener('input', () => { $('#ed-remind').disabled = !$('#ed-due').value; });
  $('#ed-nodue').addEventListener('click', () => { $('#ed-due').value = ''; $('#ed-remind').disabled = true; });
  $('#ed-save').addEventListener('click', async () => {
    if (!editing) return;
    const title = $('#ed-title').value.trim();
    if (!title) return $('#ed-title').focus();
    const due = fromLocalInput($('#ed-due').value);
    const r = $('#ed-remind').value;
    await api.update(editing, { title, notes: $('#ed-notes').value, due, remind: due != null && r !== '' ? Number(r) : null, tag: edTag });
    toast(t('Saved'));
    closeEditor();
  });
  $('#ed-delete').addEventListener('click', async () => {
    if (!editing) return;
    const id = editing;
    closeEditor();
    await api.remove(id);
    toast(t('Task deleted'));
  });

  // ---- the rest ----
  let toastTimer = null;
  function toast(text) {
    const el = $('#toast');
    el.textContent = text;
    el.hidden = false;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => { el.hidden = true; }, 1800);
  }
  document.addEventListener('click', (e) => {
    const v = e.target.closest('[data-view]');
    if (v) { e.preventDefault(); view = v.dataset.view; tagFilter = null; render(); return; }
    const g = e.target.closest('[data-tagf]');
    if (g) { e.preventDefault(); tagFilter = tagFilter === g.dataset.tagf ? null : g.dataset.tagf; if (view === 'done') view = 'all'; render(); }
  });
  $$('#sort button').forEach((b) => b.addEventListener('click', () => { sort = b.dataset.sort; render(); }));
  $('#clear-done').addEventListener('click', () => api.clearDone());
  $('#find').addEventListener('input', (e) => { query = e.target.value.trim().toLowerCase(); render(); });
  $$('[data-win]').forEach((b) => b.addEventListener('click', () => api.window(b.dataset.win)));
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && editing) closeEditor();
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey) && editing) $('#ed-save').click();
    if ((e.key === 'n' || e.key === 'N') && e.ctrlKey) { e.preventDefault(); $('#add-title').focus(); }
  });
  // the clock moves tasks from Today to Overdue and from Tomorrow to Today
  setInterval(render, 60000);

  function applyLang(l) {
    lang = l === 'ar' ? 'ar' : 'en';
    document.documentElement.lang = lang;
    document.documentElement.dir = lang === 'ar' ? 'rtl' : 'ltr';
    for (const el of $$('[data-t]')) el.textContent = t(el.dataset.t);
    for (const el of $$('[data-t-placeholder]')) el.placeholder = t(el.dataset.tPlaceholder);
    document.title = t('Tasks');
    document.documentElement.classList.remove('i18n-wait'); // i18n-early.js kept it hidden till now
    reminderOptions($('#add-remind'), 0);
    tagButtons($('#add-tag'), addTag, (g) => { addTag = g; });
  }

  // (1.8.4) the language from the address (?lang=ar), before the first paint
  if (new URLSearchParams(location.search).get('lang') === 'ar') applyLang('ar');
  api.onTasks((l) => { list = Array.isArray(l) ? l : []; render(); });
  api.data().then((d) => {
    tags = d.tags || tags;
    reminders = d.reminders || reminders;
    list = d.tasks || [];
    window.novaClock24?.(d.clock24);
    applyLang(d.lang);
    setWhen('none');
    // a first look at Today when there's something, else the whole list
    if (!list.some((x) => !x.done && x.due != null && x.due < startOfDay() + DAY)) view = list.some((x) => !x.done) ? 'all' : 'today';
    render();
    $('#add-title').focus();
  });
})();

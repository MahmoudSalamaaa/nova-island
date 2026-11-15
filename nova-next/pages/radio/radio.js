// The Radio window (1.8): every station of Quran Radio in clear groups, a search, favourites, and
// for a reciter his live station and his surahs in Mushaf order. Picking something plays it on
// the island (the island does the playing; this window only chooses).
const $ = (s) => document.querySelector(s);
const api = window.radio;

const GROUPS = [
  ['fav', 'المفضلة', '<path d="m12 3.5 2.6 5.3 5.9.9-4.3 4.1 1 5.8-5.2-2.7-5.2 2.7 1-5.8-4.3-4.1 5.9-.9Z"/>'],
  ['الإذاعات', 'الإذاعات', '<path d="M4 9.5 18 4"/><rect x="3" y="9" width="18" height="11" rx="2"/><circle cx="8.5" cy="14.5" r="2.5"/><path d="M14 13h4M14 16h4"/>'],
  ['القرّاء', 'القرّاء', '<path d="M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3Z"/><path d="M19 11a7 7 0 0 1-14 0M12 18v3"/>'],
  ['الأذكار والرقية', 'الأذكار والرقية', '<path d="M12 21s-7-4.4-7-10a4 4 0 0 1 7-2.6A4 4 0 0 1 19 11c0 5.6-7 10-7 10Z"/>'],
  ['التفسير والسيرة والحديث', 'التفسير والسيرة والحديث', '<path d="M4 5a2 2 0 0 1 2-2h13v16H6a2 2 0 0 0-2 2Z"/><path d="M4 19V5M9 7h6"/>'],
  ['تلاوات متنوعة', 'تلاوات متنوعة', '<path d="M9 18V5l11-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="17" cy="16" r="3"/>'],
  ['ترجمات المعاني', 'ترجمات المعاني', '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"/>'],
];
const STAR = '<svg viewBox="0 0 24 24"><path d="m12 3.5 2.6 5.3 5.9.9-4.3 4.1 1 5.8-5.2-2.7-5.2 2.7 1-5.8-4.3-4.1 5.9-.9Z"/></svg>';
const CHEVRON = '<svg class="go" viewBox="0 0 24 24"><path d="m15 18-6-6 6-6"/></svg>';
const PLAY = '<svg viewBox="0 0 24 24"><path d="M8 5.6v12.8a1 1 0 0 0 1.5.86l10.2-6.4a1 1 0 0 0 0-1.72L9.5 4.74A1 1 0 0 0 8 5.6Z"/></svg>';
const EQ = '<span class="eq"><i></i><i></i><i></i></span>';
const collator = new Intl.Collator('ar');

let data = { stations: [], suwar: [], now: {}, favorites: [] };
let view = { group: 'القرّاء', reciter: null };

// for search: no diacritics, one alef, ta marbuta and alef maqsura as their base letters
const fold = (s) => String(s || '').replace(/[ً-ْٰـ]/g, '').replace(/[أإآ]/g, 'ا').replace(/ة/g, 'ه').replace(/ى/g, 'ي').toLowerCase();
const el = (html) => { const t = document.createElement('template'); t.innerHTML = html.trim(); return t.content.firstElementChild; };
const isFav = (id) => data.favorites.includes(id);
const playing = (id, surah) => data.now.playing && data.now.station === id && (surah === undefined || (data.now.surah || null) === surah);

function inGroup(g) {
  const list = g === 'fav' ? data.stations.filter((s) => isFav(s.id)) : data.stations.filter((s) => s.group === g);
  return g === 'القرّاء' || g === 'ترجمات المعاني' || g === 'fav' ? [...list].sort((a, b) => collator.compare(a.name, b.name)) : list;
}

function renderSide() {
  const box = $('#groups');
  box.innerHTML = '';
  for (const [id, name, icon] of GROUPS) {
    const n = inGroup(id).length;
    if (!n && id !== 'fav') continue;
    const a = el(`<a href="#" class="${id === 'fav' ? 'fav' : ''}"><svg viewBox="0 0 24 24">${icon}</svg><span></span><span class="n">${n}</span></a>`);
    a.querySelector('span').textContent = name;
    a.classList.toggle('on', !$('#find').value.trim() && view.group === id && !view.reciter);
    a.addEventListener('click', (e) => {
      e.preventDefault();
      $('#find').value = '';
      view = { group: id, reciter: null };
      render();
      $('#content').scrollTop = 0;
    });
    box.append(a);
  }
}

function starButton(id) {
  const b = el(`<button class="star${isFav(id) ? ' on' : ''}" aria-label="المفضلة">${STAR}</button>`);
  b.title = isFav(id) ? 'إزالة من المفضلة' : 'إضافة إلى المفضلة';
  b.addEventListener('click', (e) => {
    e.stopPropagation();
    api.favorite(id, !isFav(id));
  });
  return b;
}

function stationRow(s) {
  const reciter = !!s.surahs;
  const row = el(`<div class="row st${playing(s.id) ? ' now' : ''}"><div class="txt"><b></b><small></small></div>${EQ}${reciter ? CHEVRON : ''}</div>`);
  row.prepend(starButton(s.id));
  row.querySelector('b').textContent = s.name;
  row.querySelector('small').textContent = reciter ? `${s.surahs.rewaya || 'تلاوة'} · ${s.surahs.list.length} سورة وبث مباشر` : 'بث مباشر';
  row.style.cursor = 'pointer';
  row.addEventListener('click', () => {
    if (reciter) {
      // from the search too: the search is done, and "back" leads to his group
      $('#find').value = '';
      view = { group: view.group === 'fav' ? 'fav' : s.group, reciter: s.id };
      render();
      $('#content').scrollTop = 0;
    } else {
      api.play(s.id, null);
    }
  });
  return row;
}

function renderList(title, list, empty) {
  const box = $('#content');
  box.append(el('<h1></h1>'));
  box.lastElementChild.textContent = title;
  if (!list.length) {
    box.append(el(`<div class="empty-note">${empty}</div>`));
    return;
  }
  const card = el('<div class="card"></div>');
  list.forEach((s) => card.append(stationRow(s)));
  box.append(card);
}

function renderReciter(s) {
  const box = $('#content');
  const back = el('<button class="back"><svg viewBox="0 0 24 24"><path d="m9 18 6-6-6-6"/></svg><span></span></button>');
  back.querySelector('span').textContent = (GROUPS.find((g) => g[0] === view.group) || GROUPS[2])[1];
  back.addEventListener('click', () => { view = { group: view.group, reciter: null }; render(); });
  box.append(back);
  const h = el('<h1><span></span></h1>');
  h.querySelector('span').textContent = s.name;
  const star = starButton(s.id);
  star.classList.add('head-star');
  h.append(star);
  box.append(h);
  box.append(el('<p class="lead"></p>'));
  box.lastElementChild.textContent = `${s.surahs.rewaya || 'تلاوة'} · ${s.surahs.list.length} سورة`;

  const live = el(`<button class="live-btn${playing(s.id, null) ? ' now' : ''}">${playing(s.id, null) ? EQ : '<span class="dot"></span>'}<span>بث مباشر</span></button>`);
  live.addEventListener('click', () => api.play(s.id, null));
  box.append(live);

  box.append(el('<h2>السور</h2>'));
  const grid = el('<div class="suwar"></div>');
  for (const n of s.surahs.list) {
    const b = el(`<button class="surah${playing(s.id, n) ? ' now' : ''}"><span class="num">${n}</span><b></b>${EQ}</button>`);
    b.querySelector('b').textContent = data.suwar[n - 1] || `سورة ${n}`;
    b.addEventListener('click', () => api.play(s.id, n));
    grid.append(b);
  }
  box.append(grid);
}

function render() {
  renderSide();
  const box = $('#content');
  box.innerHTML = '';
  if (!data.stations.length) {
    box.append(el('<div class="empty-note"><svg viewBox="0 0 24 24"><path d="M4 9.5 18 4"/><rect x="3" y="9" width="18" height="11" rx="2"/><circle cx="8.5" cy="14.5" r="2.5"/></svg>إذاعة القرآن الكريم متوقفة.<br>شغّلها من الإعدادات ← Plugins.</div>'));
    return;
  }
  const q = fold($('#find').value.trim());
  if (q) {
    renderList('نتائج البحث', data.stations.filter((s) => fold(s.name).includes(q)).sort((a, b) => collator.compare(a.name, b.name)), 'لا توجد نتائج.');
    return;
  }
  const r = view.reciter && data.stations.find((s) => s.id === view.reciter && s.surahs);
  if (r) {
    renderReciter(r);
    return;
  }
  const g = GROUPS.find((x) => x[0] === view.group) || GROUPS[2];
  renderList(g[1], inGroup(g[0]), 'اضغط على ★ بجانب أي قارئ أو إذاعة ليظهر هنا.');
}

$('#find').addEventListener('input', () => { view.reciter = null; render(); });
$('#find').addEventListener('keydown', (e) => { if (e.key === 'Escape') { e.target.value = ''; render(); } });
document.querySelectorAll('[data-win]').forEach((b) => b.addEventListener('click', () => api.window(b.dataset.win)));
window.addEventListener('keydown', (e) => {
  if (e.key === 'Escape' && view.reciter && !$('#find').value) { view.reciter = null; render(); }
});

api.onState((s) => {
  if (s.now) data.now = s.now;
  if (s.favorites) data.favorites = s.favorites;
  if (s.stations) data.stations = s.stations;
  render();
});
api.data().then((d) => {
  data = { ...data, ...d };
  // open where the radio is: the reciter playing, else the reciters
  const cur = data.stations.find((s) => s.id === data.now.station);
  if (cur) view = { group: cur.group, reciter: cur.surahs ? cur.id : null };
  render();
});

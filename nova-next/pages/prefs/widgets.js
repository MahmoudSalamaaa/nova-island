// Settings › Widgets (1.8): the panel for Nova's desktop widgets, the desktop being the canvas
// (as a page builder: the panel holds the pieces, the page is where they stand). Every widget and
// ready group is shown as its picture, drawn by Nova with the desktop's own code; a click puts it
// on the desktop, a click again takes it off. Where it stands is set on the desktop, by dragging;
// a widget's right-click menu removes or ungroups it there too, and this page follows.
// Text is English here; prefs-i18n.js puts the Arabic over it.
(() => {
  const page = document.getElementById('widgets');
  if (!page || !window.prefs || !window.prefs.widgets) return;
  const $ = (s) => page.querySelector(s);
  const GAP = 18;
  let data = null;

  const el = (tag, cls, text) => {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text != null) e.textContent = text;
    return e;
  };
  const pic = (id) => data.previews?.[id];
  const nameOf = (id) => data.widgets.find((w) => w.id === id)?.en || id;
  const onDesktop = () => {
    const c = data.config;
    return new Set([...c.groups.flatMap((g) => g.slots), ...c.free].filter(Boolean));
  };
  const image = (id) => {
    const img = el('img');
    if (pic(id)) img.src = pic(id).src;
    img.alt = nameOf(id);
    return img;
  };

  // the answer to a change has no pictures: the ones from the page's opening stay
  function take(d) {
    const pictures = data?.previews;
    data = d;
    data.previews = data.previews || pictures;
    render();
  }
  const act = async (a) => take(await window.prefs.doWidgets(a));

  // a group's widgets put together small, as on the desktop: rows of one wide or two small, the
  // first on the right, GAP apart
  function mini(rows, slots, width) {
    const box = el('div', 'wg-mini');
    let i = 0;
    const placed = [];
    let w = 0;
    let y = 0;
    for (const r of rows) {
      const ids = r.map(() => slots[i++]).filter((id) => id && pic(id));
      if (!ids.length) continue;
      w = Math.max(w, ids.reduce((a, id) => a + pic(id).w, 0) + GAP * (ids.length - 1));
      placed.push({ ids, y });
      y += Math.max(...ids.map((id) => pic(id).h)) + GAP;
    }
    const k = width / Math.max(w, 1);
    box.style.height = `${Math.round((y - GAP) * k)}px`;
    for (const row of placed) {
      let x = w;
      for (const id of row.ids) {
        x -= pic(id).w;
        const img = image(id);
        img.style.left = `${Math.round(x * k)}px`;
        img.style.top = `${Math.round(row.y * k)}px`;
        img.style.width = `${Math.round(pic(id).w * k)}px`;
        box.append(img);
        x -= GAP;
      }
    }
    return box;
  }

  // a widget or group; one that isn't out yet says so and can't be added
  function tile(cls, name, on, body, click, soon) {
    const b = el('button', `wg-tile ${cls}${on ? ' on' : ''}${soon ? ' soon' : ''}`);
    b.type = 'button';
    b.disabled = !!soon;
    b.setAttribute('aria-pressed', String(on));
    const foot = el('div', 'wg-tile-foot');
    foot.append(el('span', 'wg-tile-name', name), el('span', 'wg-chip', soon ? 'Coming soon' : on ? 'On desktop' : 'Add'));
    b.append(body, foot);
    if (!soon) b.addEventListener('click', click);
    return b;
  }

  function nowRow(thumbs, name, sub, buttons) {
    const row = el('div', 'wg-now-row');
    const t = el('div', 'wg-thumbs');
    t.append(...thumbs);
    const n = el('div', 'wg-now-name', name);
    if (sub) n.append(el('small', null, sub));
    row.append(t, n, ...buttons);
    return row;
  }
  function button(cls, text, click) {
    const b = el('button', cls, text);
    b.addEventListener('click', click);
    return b;
  }

  function render() {
    const c = data.config;
    $('#wg-on').checked = !!c.on;
    const shown = onDesktop();

    // what's on the desktop now: each group, each widget on its own
    const now = $('#wg-now');
    now.textContent = '';
    for (const g of c.groups) {
      const ids = g.slots.filter(Boolean);
      const ready = data.groups.find((x) => x.id === g.from);
      now.append(nowRow(ids.filter(pic).map(image), ready ? ready.en : 'Group', ids.map(nameOf).join(' · '), [
        button('btn', 'Ungroup', () => act({ ungroup: g.id })),
        button('btn danger', 'Remove', () => act({ removeGroup: g.id })),
      ]));
    }
    for (const id of c.free) {
      now.append(nowRow(pic(id) ? [image(id)] : [], nameOf(id), null, [button('btn danger', 'Remove', () => act({ remove: id }))]));
    }
    if (!c.groups.length && !c.free.length) now.append(el('div', 'wg-empty', 'Nothing on your desktop yet. Add a widget below'));

    // every widget, as it looks
    const gallery = $('#wg-gallery');
    gallery.textContent = '';
    for (const w of [...data.widgets.filter((x) => !x.soon), ...data.widgets.filter((x) => x.soon)]) {
      const on = shown.has(w.id);
      gallery.append(tile(w.slot, w.en, on, image(w.id), () => act(on ? { remove: w.id } : { add: w.id }), w.soon));
    }

    // the ready groups, put together from the same pictures
    const groups = $('#wg-groups');
    groups.textContent = '';
    for (const g of data.groups) {
      const mine = c.groups.find((x) => x.from === g.id);
      const wide = g.rows.some((r) => r.length === 1);
      groups.append(tile(wide ? 'wide' : 'small', g.en, !!mine, mini(g.rows, g.slots, wide ? 300 : 130), () => act(mine ? { removeGroup: mine.id } : { addGroup: g.id }), g.soon));
    }
  }

  $('#wg-on').addEventListener('change', () => act({ on: $('#wg-on').checked }));
  $('#wg-reset').addEventListener('click', () => act({ reset: true }));
  // the desktop changed it (a widget's menu): the page follows
  window.prefs.onWidgets?.((d) => take(d));
  window.prefs.widgets().then((d) => take(d));
})();

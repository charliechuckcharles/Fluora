/* Bouquet: the images and snaps you've picked (the flower button, or P), gathered into one
   showcase. Picking is saved with the image's marks or in the snap's record, on this computer. */
'use strict';

const BQ = { show: 'all', open: null, order: [] };

// An outline flower for "not picked"; the full Fluora flower once picked
const PICK_OUTLINE = '<svg class="pick-ol" viewBox="0 0 46 46" aria-hidden="true"><g transform="translate(23 23)">'
  + [0, 72, 144, 216, 288].map(a => `<ellipse rx="5.6" ry="11.5" cy="-9.5" transform="rotate(${a})"/>`).join('')
  + '<circle r="3.2"/></g></svg>';
const pickIcon = (on, size = 18) => on ? `<span class="pick-on">${flower(1, { size })}</span>` : PICK_OUTLINE;

const imagePicked = id => !!markOf(id).pick;
const snapPicked = x => !!x?.pick;

function pickBtn(on, act, extra = '') {
  return `<button class="pickbtn ${on ? 'on' : ''}" data-act="${act}" ${extra}
    title="${on ? 'In your Bouquet. Click to take it out' : 'Pick for your Bouquet'} (P)" aria-pressed="${on}">
    ${pickIcon(on, 18)}<span>${on ? 'In your Bouquet' : 'Pick for your Bouquet'}</span><kbd>P</kbd></button>`;
}

// A small flower in a tile's corner when it's picked
const pickBadge = on => on ? `<span class="pick-badge" title="In your Bouquet">${flower(1, { size: 14 })}</span>` : '';

async function toggleImagePick(id = S.v.id) {
  const on = !imagePicked(id);
  await setMark({ pick: on }, id).catch(e => alert(`Couldn’t save: ${e.message}`));
  toast(on ? `${flower(1, { size: 14 })} Added to your <b>Bouquet</b>` : 'Taken out of your Bouquet');
  refreshAfterPick();
}

async function toggleSnapPick(id) {
  const x = SNAPS.list.find(s => s.id === id);
  if (!x) return;
  const on = !x.pick;
  try {
    const r = await api('/api/snaps', { action: 'update', id, pick: on });
    Object.assign(x, r.snap);
  } catch (e) { alert(`Couldn’t save: ${e.message}`); return; }
  toast(on ? `${flower(1, { size: 14 })} Snap added to your <b>Bouquet</b>` : 'Snap taken out of your Bouquet');
  refreshAfterPick();
}

function refreshAfterPick() {
  if (S.v.open) renderViewer();
  if (SNAPS.open && typeof openSnap === 'function') openSnap(SNAPS.open);
  if (BQ.open) openBouquetItem(BQ.open);
  if (S.tab === 'bouquet') renderBouquet();
  else if (S.tab === 'snaps' && typeof renderSnaps === 'function') renderSnaps();
  else renderGrid();
}

// ---------------------------------------------------------------- the Bouquet tab

function bouquetItems() {
  const imgs = (S.data.slices || []).filter(s => imagePicked(s.id))
    .map(s => ({ key: `i:${s.id}`, kind: 'image', s, t: markOf(s.id).pickedAt || markOf(s.id).updated || 0 }));
  const snaps = (SNAPS.list || []).filter(snapPicked).map(x => ({ key: `s:${x.id}`, kind: 'snap', x, t: x.pickedAt || x.created || 0 }));
  const all = BQ.show === 'images' ? imgs : BQ.show === 'snaps' ? snaps : [...imgs, ...snaps];
  return all.sort((a, b) => b.t - a.t);
}

function itemTitle(it) {
  const o = it.kind === 'image' ? it.s : it.x;
  return `${slideName(o.slide)} · IM${pad(o.im)}`;
}
function itemDots(it) {
  return it.kind === 'image' ? (it.s.channels || []).map(c => dot(c.stain)).join('') : chanDots(it.x);
}
function itemText(it) {
  return it.kind === 'image' ? markOf(it.s.id).note || '' : it.x.caption || '';
}

function bqCard(it) {
  const src = it.kind === 'image' ? imgUrl(it.s.id, 'merged') : snapThumb(it.x);
  const sub = it.kind === 'image' ? [it.s.condition, it.s.block && `Block ${it.s.block}`].filter(Boolean).join(' · ')
    : [chanLabel(it.x.channel), it.x.widthUm ? `${fmtUm(it.x.widthUm)} wide` : ''].filter(Boolean).join(' · ');
  const text = itemText(it);
  return `<figure class="bq-card" data-act="bq-open" data-k="${esc(it.key)}" tabindex="0">
    <img src="${src}" alt="${esc(itemTitle(it))}" loading="lazy" draggable="false">
    ${it.kind === 'snap' ? '<span class="bq-kind">Snap</span>' : ''}
    <figcaption><b>${itemTitle(it)}</b><span class="bq-dots">${itemDots(it)}</span>
      <span class="bq-sub">${esc(sub)}</span>${text ? `<em>${esc(text)}</em>` : ''}</figcaption></figure>`;
}

// Height of a card for its width (so columns can be balanced before the images load)
function itemAspect(it) {
  const [w, h] = it.kind === 'image' ? it.s.previewSize || [1, 1] : [it.x.width || 1, it.x.height || 1];
  return h / Math.max(1, w);
}

// Columns filled shortest-first, newest picks along the top
function masonry(items) {
  const avail = Math.max(320, ($('#bouquet')?.clientWidth || innerWidth) - 64);
  const n = Math.max(1, Math.min(items.length, Math.floor((avail + 18) / (300 + 18)), 5));
  const cols = Array.from({ length: n }, () => ({ h: 0, html: [] }));
  for (const it of items) {
    const c = cols.reduce((a, b) => (b.h < a.h ? b : a));
    c.html.push(bqCard(it));
    c.h += itemAspect(it) + 0.08;
  }
  return cols.map(c => `<div class="bq-col">${c.html.join('')}</div>`).join('');
}

let bqResize;
addEventListener('resize', () => { clearTimeout(bqResize); bqResize = setTimeout(() => { if (S.tab === 'bouquet') renderBouquet(); }, 150); });

function renderBouquet() {
  const el = $('#bouquet');
  if (!el) return;
  const all = { images: (S.data.slices || []).filter(s => imagePicked(s.id)).length, snaps: (SNAPS.list || []).filter(snapPicked).length };
  const items = bouquetItems();
  const seg = (v, label, n) => `<button class="${BQ.show === v ? 'on' : ''}" data-act="bq-show" data-v="${v}">${label}<span class="n">${n}</span></button>`;
  const total = all.images + all.snaps;
  el.innerHTML = `<div class="bq-wrap">
    <div class="bq-head">
      <div class="bq-fl">${flower(Math.min(1, 0.25 + total / 12), { size: 46, bloom: true })}</div>
      <div><h1>Bouquet</h1>
        <p>${total ? `The best of this project: ${plural(all.images, 'image')} and ${plural(all.snaps, 'snap')} you’ve picked.` : 'The best of this project, all in one place.'}</p></div>
      ${total ? `<div class="seg bq-seg">${seg('all', 'All', total)}${seg('images', 'Images', all.images)}${seg('snaps', 'Snaps', all.snaps)}</div>` : ''}
    </div>
    ${items.length ? `<div class="bq-grid">${masonry(items)}</div>`
      : `<div class="bq-empty"><div class="bq-empty-fl">${flower(0, { size: 72 })}</div>
          <h2>${total ? 'Nothing here with this filter' : 'Your Bouquet is empty'}</h2>
          <p>${total ? 'Switch back to <b>All</b> to see everything you’ve picked.'
            : 'Pick the images and snaps you love and they’ll be gathered here, ready to show off. Open an image or a snap and press the flower button, or <kbd>P</kbd>.'}</p>
          ${total ? '' : '<button class="primary" data-act="tab" data-t="slides">Go to your images</button>'}</div>`}
  </div>`;
  bloomFlowers(el);
}

// ---------------------------------------------------------------- one item, full screen

function openBouquetItem(key) {
  const items = bouquetItems();
  if (!BQ.order.includes(key)) BQ.order = items.map(i => i.key);
  const it = items.find(i => i.key === key) || (() => {
    // picked out while open: keep showing it until moving on
    const [kind, id] = key.split(':');
    return kind === 'i' ? { key, kind: 'image', s: sliceById(id) } : { key, kind: 'snap', x: SNAPS.list.find(x => x.id === id) };
  })();
  if (!it || !(it.s || it.x)) { closeBouquetItem(); return; }
  BQ.open = key;
  const i = BQ.order.indexOf(key), n = BQ.order.length;
  const on = it.kind === 'image' ? imagePicked(it.s.id) : snapPicked(it.x);
  const src = it.kind === 'image' ? imgUrl(it.s.id, 'merged') : snapUrl(it.x);
  const text = itemText(it);
  const meta = it.kind === 'image'
    ? [it.s.condition, it.s.block && `Block ${it.s.block}`, (it.s.channels || []).map(c => stainName(c.stain)).join(', ')]
    : [chanLabel(it.x.channel), it.x.widthUm && `${fmtUm(it.x.widthUm)} wide`, it.x.scaleBarUm && `scale bar ${fmtUm(it.x.scaleBarUm)}`, it.x.by];
  const v = $('#bqview');
  v.innerHTML = `<div class="bqv-stage" data-act="bq-close-bg">
      <img src="${src}" alt="${esc(itemTitle(it))}" draggable="false">
      ${n > 1 ? `<button class="nav prev glass" data-act="bq-step" data-d="-1" title="Previous (←)">‹</button>
        <button class="nav next glass" data-act="bq-step" data-d="1" title="Next (→)">›</button>` : ''}
      <button class="ghost bqv-close" data-act="bq-close" title="Close (Esc)">✕</button>
    </div>
    <div class="bqv-bar">
      <div class="bqv-text"><b>${it.kind === 'snap' ? 'Snap · ' : ''}${itemTitle(it)}</b><span class="bq-dots">${itemDots(it)}</span>
        <span class="bqv-meta">${meta.filter(Boolean).map(esc).join(' · ')}${n > 1 ? ` · ${i + 1} of ${n}` : ''}</span>
        ${text ? `<p>${esc(text)}</p>` : ''}</div>
      <div class="bqv-actions">
        <button data-act="bq-go">${it.kind === 'image' ? 'Open the image' : 'Go to the image, at this spot'}</button>
        ${pickBtn(on, 'bq-pick')}
      </div>
    </div>`;
  v.hidden = false;
}

function closeBouquetItem() {
  BQ.open = null;
  BQ.order = [];
  $('#bqview').hidden = true;
  if (S.tab === 'bouquet') renderBouquet();
}

function stepBouquet(d) {
  const n = BQ.order.length;
  if (!n) return;
  const i = (BQ.order.indexOf(BQ.open) + d + n) % n;
  openBouquetItem(BQ.order[i]);
}

Object.assign(ACT, {
  pick: () => toggleImagePick(),
  'snap-pick': () => { if (SNAPS.open) toggleSnapPick(SNAPS.open); },
  'bq-show': el => { BQ.show = el.dataset.v; renderBouquet(); },
  'bq-open': el => openBouquetItem(el.dataset.k),
  'bq-close': closeBouquetItem,
  'bq-close-bg': (el) => void el,
  'bq-step': el => stepBouquet(+el.dataset.d),
  'bq-pick': () => {
    const [kind, id] = BQ.open.split(':');
    kind === 'i' ? toggleImagePick(id) : toggleSnapPick(id);
  },
  'bq-go': () => {
    const [kind, id] = BQ.open.split(':');
    closeBouquetItem();
    if (kind === 'i') { showTab('slides'); openViewer(id, [id]); }
    else { const x = SNAPS.list.find(s => s.id === id); if (x) goToSource(x); }
  },
});

document.addEventListener('keydown', e => {
  if ($('#bqview').hidden || !$('#modal').hidden || e.target.matches('textarea, input')) return;
  if (e.key === 'Escape') closeBouquetItem();
  else if (e.key === 'ArrowRight') stepBouquet(1);
  else if (e.key === 'ArrowLeft') stepBouquet(-1);
  else if (e.key.toLowerCase() === 'p') ACT['bq-pick']();
  else return;
  e.preventDefault(); e.stopImmediatePropagation();
}, true);
document.addEventListener('keydown', e => {
  // Enter opens a focused card
  if (e.key === 'Enter' && e.target.classList?.contains('bq-card')) openBouquetItem(e.target.dataset.k);
});

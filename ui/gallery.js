/* Gallery: the Good and Okay images as big pictures, four to a screen (2 × 2). Each scroll, or
   ↑ / ↓, moves one screen. Click a picture to open it in the viewer. */
'use strict';

const GAL = { show: 'both', stains: new Set(), conds: new Set(), picture: 'merged' };
// show: 'both' | 'good' | 'ok'; stains / conds: none picked = all; picture: 'merged' | 'channel'

const galMark = (s, show = GAL.show) => { const st = markOf(s.id).status; return show === 'both' ? st === 'good' || st === 'ok' : st === show; };
const galStain = s => !GAL.stains.size || (s.channels || []).some(c => GAL.stains.has(c.stain));
const galCond = s => !GAL.conds.size || GAL.conds.has(s.condition);
function galleryList() { return S.data.slices.filter(s => galMark(s) && galStain(s) && galCond(s)); }
// "Channel only": the first picked stain this image has (picked stains are kept in the order you clicked them)
const galChannel = s => GAL.picture === 'channel' && [...GAL.stains].find(st => (s.channels || []).some(c => c.stain === st));

function renderGallery() {
  const el = $('#gallery');
  const old = $('#gal-scroll'), keepTop = old ? old.scrollTop : 0;
  const list = galleryList();
  const n = show => S.data.slices.filter(s => galMark(s, show) && galStain(s) && galCond(s)).length;
  const seg = (k, l) => `<button class="${GAL.show === k ? 'on' : ''}" data-act="gal-show" data-v="${k}">${l}<span class="n">${n(k)}</span></button>`;
  // Stains on the Good / Okay images showing, each with how many of them have it
  const marked = S.data.slices.filter(s => galMark(s) && galCond(s));
  const stains = [...new Set(S.data.slices.flatMap(s => (s.channels || []).map(c => c.stain)))]
    .map(st => [st, marked.filter(s => (s.channels || []).some(c => c.stain === st)).length]);
  const forConds = S.data.slices.filter(s => galMark(s) && galStain(s));
  const conds = [...new Set(S.data.slices.map(s => s.condition).filter(Boolean))].sort();
  const condChips = conds.map(c => `<button class="chip cond-${esc(c)} ${GAL.conds.has(c) ? 'on' : ''}" data-act="gal-cond" data-v="${esc(c)}">${esc(c)}<span class="n">${forConds.filter(s => s.condition === c).length}</span></button>`).join('');
  const chips = stains.map(([st, c]) => `<button class="chip ${GAL.stains.has(st) ? 'on' : ''}" data-act="gal-stain" data-v="${esc(st)}" ${c ? '' : 'disabled'}>${dot(st)}${esc(stainShort(st))}<span class="n">${c}</span></button>`).join('');
  const pages = [];
  for (let i = 0; i < list.length; i += 4) pages.push(list.slice(i, i + 4));
  el.innerHTML = `<div class="gal-bar">
      <div class="seg gal-seg">${seg('both', 'Good + Okay')}${seg('good', '✓ Good')}${seg('ok', '~ Okay')}</div>
      <div class="chips gal-stains">${chips}${GAL.stains.size ? '<button class="chip" data-act="gal-stain" data-v="">Any stain</button>' : ''}</div>
      <div class="chips gal-stains">${condChips}</div>
      <div class="seg gal-seg" title="${GAL.stains.size ? '' : 'Pick a stain first to see just its channel'}">
        <button class="${GAL.picture === 'merged' || !GAL.stains.size ? 'on' : ''}" data-act="gal-picture" data-v="merged">Merged</button>
        <button class="${GAL.picture === 'channel' && GAL.stains.size ? 'on' : ''}" data-act="gal-picture" data-v="channel" ${GAL.stains.size ? '' : 'disabled'}>${
          GAL.stains.size ? esc([...GAL.stains].map(stainShort).join(' / ')) + ' only' : 'Channel only'}</button></div>
      <span class="gal-page mono" id="gal-page"></span>
      <span class="gal-hint">Scroll or press <kbd>↑</kbd> <kbd>↓</kbd> for the next four · Click a picture to open it</span></div>
    <div class="gal-scroll" id="gal-scroll">${pages.length ? pages.map(p => `<section class="gal-pagebox">${p.map(galCell).join('')}</section>`).join('')
      : `<div class="empty"><div><div class="big">${GAL.stains.size ? 'No images with that stain here' : `Nothing marked ${GAL.show === 'ok' ? 'okay' : GAL.show === 'good' ? 'good' : 'good or okay'} yet`}</div>
         <div>${GAL.stains.size ? 'Pick another stain, or click <b>Any stain</b>.' : 'Mark images with <kbd>G</kbd> or <kbd>O</kbd> in the viewer and they show up here.'}</div></div></div>`}</div>`;
  const sc = $('#gal-scroll');
  sc.scrollTop = keepTop;
  sc.onscroll = galPageLabel;
  sc.onwheel = galWheel;
  galPageLabel();
}

// One scroll gesture (a wheel click or a trackpad swipe, however long it coasts) = one screen of four
let galLock = false, galQuiet;
function galGo(d) {
  const sc = $('#gal-scroll');
  if (!sc) return;
  const total = sc.querySelectorAll('.gal-pagebox').length;
  const i = Math.max(0, Math.min(total - 1, Math.round(sc.scrollTop / sc.clientHeight) + d));
  sc.scrollTo({ top: i * sc.clientHeight, behavior: 'smooth' });
}
function galWheel(e) {
  if (e.ctrlKey || Math.abs(e.deltaY) < Math.abs(e.deltaX)) return;  // pinch-zoom or sideways: leave alone
  e.preventDefault();
  clearTimeout(galQuiet);
  galQuiet = setTimeout(() => { galLock = false; }, 220);
  if (galLock || Math.abs(e.deltaY) < 4) return;
  galLock = true;
  galGo(Math.sign(e.deltaY));
}

function galCell(s) {
  const m = markOf(s.id);
  const ch = galChannel(s);
  return `<figure class="gal-cell ${m.status}" data-act="gal-open" data-id="${s.id}" ${m.note ? `title="${esc(m.note)}"` : ''}>
    <img src="${imgUrl(s.id, ch || 'merged')}" alt="${slideName(s.slide)} image ${s.im}" loading="lazy" draggable="false">${typeof pickBadge === 'function' ? pickBadge(m.pick) : ''}
    <figcaption><b>${slideName(s.slide)} · IM${pad(s.im)}</b>${ch ? `<span class="gal-ch">${dot(ch)}${esc(stainName(ch))} only</span>` : ''}<span>${esc([s.block, s.condition].filter(Boolean).join(' · '))}</span>
      ${pillFor(m.status)}${m.note ? `<em>✎ ${esc(m.note)}</em>` : ''}</figcaption></figure>`;
}

function galPageLabel() {
  const sc = $('#gal-scroll'), label = $('#gal-page');
  if (!sc || !label) return;
  const total = sc.querySelectorAll('.gal-pagebox').length;
  label.textContent = total ? `${Math.min(total, Math.round(sc.scrollTop / sc.clientHeight) + 1)} of ${total}` : '';
}

Object.assign(ACT, {
  'gal-show': el => { GAL.show = el.dataset.v; renderGallery(); $('#gal-scroll').scrollTop = 0; galPageLabel(); },
  'gal-stain': el => {
    const v = el.dataset.v;
    if (!v) GAL.stains.clear(); else GAL.stains.has(v) ? GAL.stains.delete(v) : GAL.stains.add(v);
    renderGallery(); $('#gal-scroll').scrollTop = 0; galPageLabel();
  },
  'gal-cond': el => { const v = el.dataset.v; GAL.conds.has(v) ? GAL.conds.delete(v) : GAL.conds.add(v); renderGallery(); $('#gal-scroll').scrollTop = 0; galPageLabel(); },
  'gal-picture': el => { GAL.picture = el.dataset.v; renderGallery(); },
  'gal-open': el => {
    const ch = galChannel(sliceById(el.dataset.id));
    openViewer(el.dataset.id, galleryList().map(s => s.id));
    if (ch) { S.v.channel = ch; renderViewer(); }  // open on the same channel you were looking at
  },
});

document.addEventListener('keydown', e => {
  if (S.tab !== 'gallery' || S.v.open || !$('#modal').hidden || e.target.matches?.('input, textarea')) return;
  const sc = $('#gal-scroll'), d = { ArrowDown: 1, PageDown: 1, ' ': 1, ArrowUp: -1, PageUp: -1 }[e.key];
  if (!sc || !d) return;
  e.preventDefault();
  galGo(d);
});
addEventListener('resize', () => S.tab === 'gallery' && galPageLabel());

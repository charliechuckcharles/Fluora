/* Snaps: zoom in on an image in the viewer, press S (or the camera button), and Fluora saves
   exactly that view as a PNG with a scale bar burned in. When the lab drive is connected the snap
   is made from the scanner's full-resolution files, so it is sharper than the screen.
   Every snap remembers its image, stain, the spot on the image and the pixel size, so it can always
   be traced back ("Go to the image"), and that information is also stored inside the PNG itself. */
'use strict';

const CAMERA_SVG = '<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 8h3l1.6-2.2A1 1 0 0 1 9.4 5.4h5.2a1 1 0 0 1 .8.4L17 8h3a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V9a1 1 0 0 1 1-1z"/><circle cx="12" cy="13" r="3.6"/></svg>';
const SNAPS = { list: [], f: { slide: null, channel: null, who: null, q: '' }, open: null, order: [] };
const snapsOf = id => SNAPS.list.filter(x => x.slice === id);
const snapUrl = x => `/snap/${encodeURIComponent(x.file)}`;
const snapThumb = x => `/snap/${x.thumb.split('/').map(encodeURIComponent).join('/')}`;
const fmtUm = um => um == null ? '' : um >= 1000 ? `${+(um / 1000).toFixed(2)} mm` : `${um >= 10 ? Math.round(um) : +um.toFixed(1)} µm`;
const fmtDate = t => new Date(t * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });
const chanLabel = c => c === 'merged' ? 'Merged' : stainName(c);
const chanDots = x => x.channel === 'merged' ? (x.stains || []).map(st => dot(st)).join('') : dot(x.channel);

async function loadSnaps() {
  try { SNAPS.list = await api('/api/snaps'); } catch { SNAPS.list = []; }
  if (S.tab === 'snaps') renderSnaps();
  if (S.data.slices.length) renderGrid();
  if (S.v.open) renderViewer();
}

function toast(html, ms = 3500) {
  const t = $('#toast');
  t.innerHTML = html; t.hidden = false; t.classList.remove('out');
  clearTimeout(toast.timer);
  if (ms) toast.timer = setTimeout(() => { t.classList.add('out'); setTimeout(() => { t.hidden = true; }, 250); }, ms);
}

// ---------------------------------------------------------------- in the viewer

function snapBadge(id) {
  const n = snapsOf(id).length;
  return n ? `<span class="snapn" title="${n} snap${n === 1 ? '' : 's'}">${CAMERA_SVG}${n}</span>` : '';
}

function viewerSnapsHtml(s) {
  const mine = snapsOf(s.id);
  return `<section><div class="label">Snaps of this image${mine.length ? `<span class="mono">${mine.length}</span>` : ''}</div>
    <button class="board-add snap-take" data-act="snap">${CAMERA_SVG}Snap this view<kbd class="k">S</kbd></button>
    ${mine.length ? `<div class="vsnaps">${mine.map(x => `<button class="vsnap" data-act="snap-open" data-id="${x.id}" title="${esc(x.caption || chanLabel(x.channel))}">
      <img src="${snapThumb(x)}" alt="" loading="lazy" draggable="false"></button>`).join('')}</div>`
      : '<p class="fine">Zoom in on something worth keeping and snap it. The snap gets a scale bar and remembers exactly where it came from.</p>'}</section>`;
}

let snapping = false;
async function takeSnap() {
  if (!S.v.open || snapping) return;
  const s = current(), st = $('#stage'), img = $('#vimg');
  if (!s || !img?.naturalWidth) return;
  snapping = true;
  try {
    const nw = img.naturalWidth, nh = img.naturalHeight, k = S.v.scale;
    // The part of the image that is on screen, in preview pixels
    const x0 = Math.max(0, -S.v.x / k), y0 = Math.max(0, -S.v.y / k);
    const x1 = Math.min(nw, (st.clientWidth - S.v.x) / k), y1 = Math.min(nh, (st.clientHeight - S.v.y) / k);
    if (x1 - x0 < 4 || y1 - y0 < 4) { toast('Nothing of the image is on screen to snap.'); return; }
    const frac = { fx: x0 / nw, fy: y0 / nh, fw: (x1 - x0) / nw, fh: (y1 - y0) / nh };
    const screenLong = Math.max(x1 - x0, y1 - y0) * k * (devicePixelRatio || 1);
    const want = Math.min(3000, Math.max(2000, screenLong));
    st.classList.add('flash'); setTimeout(() => st.classList.remove('flash'), 350);
    toast(`<span class="spin"></span>Making the snap…`, 0);
    const r = await fetch('/api/snap-region', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: s.id, channel: S.v.channel, ...frac, want, bright: S.v.bright }) });
    if (!r.ok) throw new Error((await r.json().catch(() => ({}))).error || r.statusText);
    const source = r.headers.get('X-Snap-Source'), level = +r.headers.get('X-Snap-Level');
    const umNative = parseFloat(r.headers.get('X-Snap-Um')) || null;
    const bmp = await createImageBitmap(await r.blob());
    // Never smaller than what was on screen, never bigger than 4000 px across
    const long = Math.max(bmp.width, bmp.height);
    const scale = long < screenLong ? screenLong / long : long > 4000 ? 4000 / long : 1;
    const W = Math.round(bmp.width * scale), H = Math.round(bmp.height * scale);
    const cv = document.createElement('canvas'); cv.width = W; cv.height = H;
    const ctx = cv.getContext('2d');
    ctx.imageSmoothingEnabled = true; ctx.imageSmoothingQuality = 'high';
    ctx.drawImage(bmp, 0, 0, W, H);
    const umPerPx = umNative ? umNative / scale : null;
    const barUm = umPerPx ? drawScaleBar(ctx, W, H, umPerPx) : null;
    // sent as text: the app window drops binary uploads to its own address
    const png = cv.toDataURL('image/png').split(',')[1];
    const [fullW, fullH] = fullSizeOf(s);
    const now = new Date(), p2 = n => String(n).padStart(2, '0');
    const meta = { slice: s.id, slide: s.slide, im: s.im, block: s.block, cond: s.cond, condition: s.condition, stains: (s.channels || []).map(c => c.stain),
      channel: S.v.channel, channelLabel: chanLabel(S.v.channel), region: frac, width: W, height: H, umPerPx, scaleBarUm: barUm,
      widthUm: s.umPerPx ? frac.fw * fullW * s.umPerPx : null, heightUm: s.umPerPx ? frac.fh * fullH * s.umPerPx : null,
      level, source, bright: S.v.bright, contrast: Object.fromEntries((s.channels || []).map(c => [c.stain, S.data.contrast[c.stain]])),
      fullResFile: (s.levels['1'] || '').split(/[\\/]/).pop(),
      stamp: `${now.getFullYear()}${p2(now.getMonth() + 1)}${p2(now.getDate())}-${p2(now.getHours())}${p2(now.getMinutes())}${p2(now.getSeconds())}` };
    const j = await api('/api/snap-save', { png, meta });
    SNAPS.list.push(j.snap);
    renderViewer();
    const where = source === 'full' ? `sharp, from the R${level} file` : source === 'preview' ? 'preview quality. Connect the lab drive for a sharper snap' : '';
    toast(`<b>Snapped</b> · ${W.toLocaleString()} × ${H.toLocaleString()} px${where ? ` · ${where}` : ''} <button class="link" data-act="snap-open" data-id="${j.snap.id}">Open</button>`, 6000);
  } catch (e) {
    toast(`<span class="err">Couldn’t make the snap: ${esc(e.message)}</span>`, 7000);
  } finally { snapping = false; }
}

// A white scale bar with its length above it, bottom right, sized to the picture
function drawScaleBar(ctx, W, H, umPerPx) {
  const target = W * 0.18 * umPerPx;
  const nice = [0.5, 1, 2, 5, 10, 20, 25, 50, 100, 200, 250, 500, 1000, 2000, 5000];
  const um = [...nice].reverse().find(v => v <= target) || nice[0];
  const len = um / umPerPx, unit = Math.max(W, H);
  const m = Math.round(unit * 0.03), bh = Math.max(4, Math.round(unit * 0.0065)), fs = Math.max(14, Math.round(unit * 0.022));
  const x = W - m - len, y = H - m - bh;
  ctx.save();
  ctx.shadowColor = 'rgba(0,0,0,.75)'; ctx.shadowBlur = Math.max(3, fs * 0.25);
  ctx.fillStyle = '#fff';
  ctx.fillRect(Math.round(x), y, Math.round(len), bh);
  ctx.font = `600 ${fs}px Inter, -apple-system, "Segoe UI", sans-serif`;
  ctx.textAlign = 'center'; ctx.textBaseline = 'bottom';
  ctx.fillText(fmtUm(um), x + len / 2, y - Math.round(fs * 0.35));
  ctx.restore();
  return um;
}

// Opening an image from a snap shows the same spot, at the same zoom, in the same channel
function goToSource(x) {
  closeSnap();
  const s = sliceById(x.slice);
  if (!s) { alert('That image isn’t in this project’s slide list any more.'); return; }
  S.v.channel = x.channel;
  S.v.bright = x.bright || 1;
  S.v.region = x.region;
  if (S.tab !== 'slides') showTab('slides');
  openViewer(x.slice, filtered().some(f => f.id === x.slice) ? filtered().map(f => f.id) : [x.slice]);
}
function zoomToRegion() {
  const r = S.v.region, st = $('#stage'), img = $('#vimg');
  S.v.region = null;
  if (!r || !img?.naturalWidth) return fit();
  const nw = img.naturalWidth, nh = img.naturalHeight;
  const sc = Math.min(st.clientWidth / (r.fw * nw), st.clientHeight / (r.fh * nh));
  Object.assign(S.v, { scale: sc, x: st.clientWidth / 2 - (r.fx + r.fw / 2) * nw * sc, y: st.clientHeight / 2 - (r.fy + r.fh / 2) * nh * sc });
  applyTransform();
}

// ---------------------------------------------------------------- the Snaps tab

function filteredSnaps() {
  const f = SNAPS.f, q = f.q.trim().toLowerCase();
  return SNAPS.list.filter(x =>
    (f.slide == null || x.slide === f.slide) && (!f.channel || x.channel === f.channel) && (!f.who || x.by === f.who) &&
    (!q || [x.caption, x.file, `slide ${x.slide}`, x.condition, x.block, chanLabel(x.channel)].join(' ').toLowerCase().includes(q)))
    .sort((a, b) => b.created - a.created);
}

function renderSnaps() {
  const el = $('#snaps');
  const all = SNAPS.list, list = filteredSnaps(), f = SNAPS.f;
  SNAPS.order = list.map(x => x.id);
  const slidesN = [...new Set(all.map(x => x.slide))].sort((a, b) => a - b);
  const chans = [...new Set(all.map(x => x.channel))];
  const people = [...new Set(all.map(x => x.by).filter(Boolean))];
  const c = (g, v, label, on) => `<button class="chip ${on ? 'on' : ''}" data-act="snap-filter" data-g="${g}" data-v="${esc(v ?? '')}">${label}</button>`;
  el.innerHTML = `<div class="layout"><aside>
      <h4>Search</h4><input type="search" class="tray-q" data-in="snap-q" placeholder="Captions, slide, stain…" value="${esc(f.q)}">
      <h4>Channel</h4><div class="chips">${c('channel', '', 'All', !f.channel)}${chans.map(ch => c('channel', ch, (ch === 'merged' ? '' : dot(ch)) + esc(ch === 'merged' ? 'Merged' : stainShort(ch)), f.channel === ch)).join('')}</div>
      ${people.length > 1 ? `<h4>Taken by</h4><div class="chips">${c('who', '', 'Everyone', !f.who)}${people.map(p => c('who', p, esc(p), f.who === p)).join('')}</div>` : ''}
      <h4>Slide</h4><div class="slide-list">
        <button class="slide-btn ${f.slide == null ? 'on' : ''}" data-act="snap-filter" data-g="slide" data-v=""><span class="num">All</span><span class="meta">Every slide</span><span class="count">${all.length}</span></button>
        ${slidesN.map(n => { const ss = all.filter(x => x.slide === n);
          return `<button class="slide-btn ${f.slide === n ? 'on' : ''}" data-act="snap-filter" data-g="slide" data-v="${n}"><span class="num">SL${pad(n)}</span>
            <span class="meta">${esc([ss[0].block, ss[0].condition].filter(Boolean).join(' · '))}</span><span class="count">${ss.length}</span></button>`; }).join('')}</div>
      <p class="note">Snaps are saved as PNG files on this computer. Each file also carries its slide, image, stain and scale inside it.</p>
      <button class="ghost wide" data-act="snap-folder">Open the snaps folder</button>
    </aside><main>
    ${!all.length ? `<div class="empty"><div><div class="snap-empty-icon">${CAMERA_SVG}</div><div class="big">No snaps yet</div>
      <div>Open an image, zoom in on something worth keeping, and press <kbd>S</kbd> or the <b>Snap</b> button.
      You get a picture of exactly that view with a scale bar, linked back to its slide and image.</div></div></div>`
    : !list.length ? '<div class="empty"><div><div class="big">Nothing matches</div><div>Try clearing a filter on the left.</div></div></div>'
    : `<div class="snap-head"><b>${list.length}</b> snap${list.length === 1 ? '' : 's'}${list.length !== all.length ? ` of ${all.length}` : ''}</div>
      <div class="snap-grid">${list.map(snapCard).join('')}</div>`}
    </main></div>`;
}

function snapCard(x) {
  return `<article class="snap-card" data-act="snap-open" data-id="${x.id}" draggable="false">
    <div class="sc-img"><img src="${snapThumb(x)}" alt="" loading="lazy" draggable="false">${typeof pickBadge === 'function' ? pickBadge(x.pick) : ''}</div>
    <div class="sc-body">
      <div class="sc-title"><b>${slideName(x.slide)} · IM${pad(x.im)}</b><span class="sc-chan">${chanDots(x)}${esc(chanLabel(x.channel))}</span></div>
      ${x.caption ? `<p class="sc-cap">${esc(x.caption)}</p>` : ''}
      <div class="sc-meta">${[x.widthUm ? fmtUm(x.widthUm) + ' wide' : '', x.condition, x.by, fmtDate(x.created)].filter(Boolean).map(esc).join(' · ')}</div>
    </div></article>`;
}

// ---------------------------------------------------------------- one snap, large

function openSnap(id) {
  const x = SNAPS.list.find(x => x.id === id);
  if (!x) return;
  SNAPS.open = id;
  if (!SNAPS.order.includes(id)) SNAPS.order = (S.v.open ? snapsOf(x.slice) : filteredSnaps()).map(x => x.id);
  const i = SNAPS.order.indexOf(id), n = SNAPS.order.length;
  const v = $('#snapview');
  const dl = (k, val) => val ? `<dt>${k}</dt><dd>${val}</dd>` : '';
  v.innerHTML = `<div class="sv-stage">
      <img src="${snapUrl(x)}" alt="" draggable="false">
      <div class="topbar"><div class="title glass"><b>${slideName(x.slide)}</b> · IM${pad(x.im)} · ${esc(chanLabel(x.channel))} ${n > 1 ? `<span class="muted">· ${i + 1} of ${n}</span>` : ''}</div></div>
      ${n > 1 ? `<button class="nav prev glass" data-act="snap-step" data-d="-1" title="Previous (←)">‹</button><button class="nav next glass" data-act="snap-step" data-d="1" title="Next (→)">›</button>` : ''}
    </div>
    <div class="side">
      <div class="side-head"><div><h2>Snap <span class="mono">${slideName(x.slide)} · IM${pad(x.im)}</span></h2>
        <div class="sub">${x.cond ? `<span class="tag cond-${esc(x.condition)}">${esc(x.condition)}</span>` : ''}<span class="tag">${chanDots(x)}${esc(chanLabel(x.channel))}</span></div></div>
        <button class="ghost" data-act="snap-close" title="Close (Esc)">✕</button></div>
      <section>${typeof pickBtn === 'function' ? pickBtn(snapPicked(x), 'snap-pick') : ''}</section>
      <section><div class="label">Caption</div>
        <textarea data-in="snap-caption" placeholder="What does this show? e.g. hyphal bodies in the abdomen">${esc(x.caption || '')}</textarea></section>
      <section><button class="primary wide" data-act="snap-source">Go to the image, at this spot</button>
        <div class="open" style="margin-top:8px">
          <button data-act="snap-board">＋ Add to board</button>
          <button data-act="snap-fiji">Open in Fiji</button>
          <button data-act="snap-reveal">${IS_WIN ? 'Show in File Explorer' : 'Show in Finder'}</button></div></section>
      <section><div class="label">Details</div><dl>
        ${dl('Slide', `${x.slide}${x.block ? ` <span class="muted">(block ${esc(x.block)})</span>` : ''}`)}
        ${dl('Image', `IM${pad(x.im)}`)}
        ${dl('Condition', esc(x.condition || ''))}
        ${dl('Channel', esc(chanLabel(x.channel)))}
        ${dl('Area', x.widthUm ? `${fmtUm(x.widthUm)} × ${fmtUm(x.heightUm)}` : '')}
        ${dl('Scale bar', fmtUm(x.scaleBarUm))}
        ${dl('Pixel size', x.umPerPx ? `${x.umPerPx.toFixed(3)} µm` : '')}
        ${dl('Size', `${x.width.toLocaleString()} × ${x.height.toLocaleString()} px`)}
        ${dl('Made from', x.source === 'full' ? `R${x.level} full-resolution file` : `R${x.level} preview${x.source === 'preview' ? ' <span class="muted">(drive not connected)</span>' : ''}`)}
        ${dl('Taken by', esc(x.by || ''))}
        ${dl('Taken', fmtDate(x.created))}
        ${dl('File', `<span class="mono" style="font-size:12px;word-break:break-all">${esc(x.file)}</span>`)}
      </dl></section>
      <section><button class="ghost danger" data-act="snap-remove">Remove this snap</button></section>
    </div>`;
  v.hidden = false;
}
function closeSnap() { flushCaption(); SNAPS.open = null; $('#snapview').hidden = true; if (S.tab === 'snaps') renderSnaps(); if (S.v.open) renderViewer(); }
function stepSnap(d) {
  flushCaption();
  const o = SNAPS.order, i = o.indexOf(SNAPS.open);
  if (o.length > 1) openSnap(o[(i + d + o.length) % o.length]);
}
const openSnapRec = () => SNAPS.list.find(x => x.id === SNAPS.open);

let capTimer, pendingCap = null;
function flushCaption() {
  clearTimeout(capTimer);
  if (!pendingCap) return;
  const { id, caption } = pendingCap; pendingCap = null;
  api('/api/snaps', { action: 'update', id, caption }).catch(e => alert(`Couldn’t save the caption: ${e.message}`));
}

// ---------------------------------------------------------------- the Board

function snapCardHtml(c, del) {
  const x = SNAPS.list.find(x => x.id === c.snap);
  if (!x) return `<div class="bcard missing" data-card="${c.id}" style="left:${c.x}px;top:${c.y}px"><div class="bimg">This snap was removed</div>${del}</div>`;
  return `<div class="bcard snapc" data-card="${c.id}" style="left:${c.x}px;top:${c.y}px" title="${esc(x.caption || '')}">
    <div class="bimg"><img src="${snapThumb(x)}" alt="" draggable="false"><span class="snapflag">${CAMERA_SVG}</span></div>
    <div class="bcap"><b>${slideName(x.slide)} · IM${pad(x.im)}</b><span class="sc-chan">${chanDots(x)}</span></div>
    <div class="bsub">${esc(x.caption || [chanLabel(x.channel), x.widthUm ? fmtUm(x.widthUm) + ' wide' : ''].filter(Boolean).join(' · '))}</div>${del}</div>`;
}
function snapTrayList(q) {
  const list = [...SNAPS.list].filter(x => !q || String(x.slide) === q).sort((a, b) => b.created - a.created);
  if (!list.length) return `<p class="note">No snaps${q ? ' of that slide' : ''} yet. Take them in the viewer with <kbd>S</kbd>.</p>`;
  return list.map(x => `<div class="tray-item" draggable="true" data-snap="${x.id}" title="${esc(x.caption || 'Drag onto the board')}">
    <img src="${snapThumb(x)}" alt="" loading="lazy" draggable="false">
    <div><b>${slideName(x.slide)} · IM${pad(x.im)}</b><span>${esc(chanLabel(x.channel))}${x.widthUm ? ' · ' + fmtUm(x.widthUm) : ''}</span>
    ${x.caption ? `<span class="tn">${esc(x.caption)}</span>` : ''}${BOARD.cards.some(c => c.snap === x.id) ? '<span class="onb">On the board</span>' : ''}</div></div>`).join('');
}
function addSnapToBoard(x) {
  const v = BOARD.view, w = innerWidth - 300, h = innerHeight - 64, n = BOARD.cards.length % 12;
  BOARD.cards.push({ id: uid('c'), snap: x.id, x: Math.round((w / 2 - v.x) / v.k - CARD_W / 2 + (n % 4) * 24), y: Math.round((h / 2 - v.y) / v.k - CARD_H / 2 + n * 18) });
  saveBoard();
  toast('Added to the board ✓', 2000);
}

// ---------------------------------------------------------------- events

Object.assign(ACT, {
  snap: () => takeSnap(),
  'snap-open': el => openSnap(el.dataset.id),
  'snap-close': closeSnap,
  'snap-step': el => stepSnap(+el.dataset.d),
  'snap-source': () => { const x = openSnapRec(); if (x) goToSource(x); },
  'snap-board': () => { const x = openSnapRec(); if (x) addSnapToBoard(x); },
  'snap-reveal': () => api('/api/snaps', { action: 'reveal', id: SNAPS.open }).catch(e => alert(e.message)),
  'snap-fiji': () => api('/api/snaps', { action: 'fiji', id: SNAPS.open }).catch(e => alert(e.message)),
  'snap-folder': () => api('/api/snaps', { action: 'folder' }).catch(e => alert(e.message)),
  'snap-remove': async () => {
    const x = openSnapRec();
    if (!x || !confirm('Remove this snap?\n\nIts PNG moves to a “removed” folder inside the snaps folder, so it can be brought back.')) return;
    pendingCap = null;
    await api('/api/snaps', { action: 'remove', id: x.id }).catch(e => alert(e.message));
    SNAPS.list = SNAPS.list.filter(y => y.id !== x.id);
    SNAPS.order = SNAPS.order.filter(id => id !== x.id);
    closeSnap(); renderGrid();
  },
  'snap-filter': el => {
    const { g, v } = el.dataset;
    SNAPS.f[g] = v === '' ? null : g === 'slide' ? +v : v;
    renderSnaps();
  },
});

document.addEventListener('input', e => {
  const k = e.target.dataset?.in;
  if (k === 'snap-q') {
    SNAPS.f.q = e.target.value;
    const pos = e.target.selectionStart; renderSnaps();
    const i = document.querySelector('[data-in="snap-q"]'); i.focus(); i.setSelectionRange(pos, pos);
  }
  if (k === 'snap-caption') {
    const x = openSnapRec(); if (!x) return;
    x.caption = e.target.value;
    clearTimeout(capTimer); pendingCap = { id: x.id, caption: x.caption }; capTimer = setTimeout(flushCaption, 500);
  }
});
document.addEventListener('keydown', e => {
  if ($('#snapview').hidden || !$('#modal').hidden || e.target.matches('textarea, input')) return;
  if (e.key === 'Escape') closeSnap();
  else if (e.key.toLowerCase() === 'p' && typeof ACT['snap-pick'] === 'function') ACT['snap-pick']();
  else if (e.key === 'ArrowRight') stepSnap(1);
  else if (e.key === 'ArrowLeft') stepSnap(-1);
  else if (e.metaKey || e.ctrlKey) return;  // let copy etc. through
  e.preventDefault(); e.stopImmediatePropagation();  // keep keys away from the image viewer underneath
}, true);
document.addEventListener('dragstart', e => {
  const it = e.target.closest?.('.tray-item[data-snap]');
  if (!it) return;
  e.dataTransfer.setData('text/x-snap', it.dataset.snap);
  e.dataTransfer.effectAllowed = 'copy';
});

loadSnaps();
// Tell the log what this window can do (it has no console to look at)
api('/api/log', { msg: `window: secure=${window.isSecureContext} clipboard=${!!navigator.clipboard} origin=${location.origin}` }).catch(() => {});

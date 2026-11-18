/* Fluora — grid of every image, a viewer to click through them, and good / not-good marks. */
'use strict';

/* Stain colours come from the server (slides.STAIN_COLOR), the same list that colours the merged
   images, so the dots always match what the image shows. */
const stainColor = st => (S.data.stainColors || {})[st] || '#fff';
const STAIN_LABEL = { DAPI: 'DAPI', CALCOFLUOR: 'Calcofluor', GAF: 'Green autofluorescence', SYTOX: 'SYTOX',
  PHALLOIDIN: 'Phalloidin', PI: 'Propidium iodide' };
const STAIN_SHORT = { GAF: 'GAF', PI: 'PI' };

const S = {
  data: { slices: [], marks: {}, contrast: {}, build: {}, settings: {}, apps: {} },
  f: { cond: new Set(), stain: new Set(), slide: null, mark: 'all' },
  v: { open: false, id: null, channel: 'merged', bright: 1, scale: 0, x: 0, y: 0 },
};
const IS_WIN = /Win/.test(navigator.userAgentData?.platform || navigator.platform);
const $ = sel => document.querySelector(sel);
const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const pad = n => String(n).padStart(2, '0');

async function api(path, body) {
  const r = await fetch(path, body ? { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) } : {});
  const j = await r.json();
  if (!r.ok) throw new Error(j.error || r.statusText);
  return j;
}
// The app window has no console to read, so problems go to app.log in the app's data folder
addEventListener('error', e => { api('/api/log', { msg: `error: ${e.message} (${(e.filename || '').split('/').pop()}:${e.lineno})` }).catch(() => {}); });
addEventListener('unhandledrejection', e => { api('/api/log', { msg: `error: ${e.reason?.message || e.reason}` }).catch(() => {}); });
const markOf = id => S.data.marks[id] || {};
const imgUrl = (id, kind) => `/img/${id}_${kind}.jpg?v=${S.data.builtAt || 0}`;
const stainName = s => STAIN_LABEL[s] || s;
const stainShort = s => STAIN_SHORT[s] || stainName(s);
const dot = st => `<span class="dot" style="color:${stainColor(st)};background:${stainColor(st)}"></span>`;

function filtered() {
  const f = S.f;
  return S.data.slices.filter(s =>
    (!f.cond.size || f.cond.has(s.condition)) &&
    (!f.stain.size || (s.channels || []).some(c => f.stain.has(c.stain))) &&
    (f.slide == null || s.slide === f.slide) &&
    (f.mark === 'all' || (f.mark === 'unmarked' ? !markOf(s.id).status : f.mark === 'note' ? !!markOf(s.id).note : markOf(s.id).status === f.mark)));
}

// ---------------------------------------------------------------- header / filters / grid

function renderBuild() {
  const b = S.data.build || {};
  const el = $('#build');
  if (b.phase === 'error') el.innerHTML = `<span class="err">⚠ ${esc(b.error)}</span>`;
  else if (b.phase && !['idle', 'done'].includes(b.phase)) {
    const pct = b.total ? Math.round(b.done / b.total * 100) : 0;
    el.innerHTML = `${esc({ scanning: 'Searching image folder', copying: 'Copying previews', measuring: 'Measuring', rendering: 'Making images' }[b.phase] || b.phase)}
      <span class="bar"><i style="width:${pct}%"></i></span><span class="mono">${b.done}/${b.total}</span>`;
  } else el.innerHTML = S.data.labConnected === false ? `<span class="off" title="Can’t reach ${esc(S.data.settings.imageFolder)}. If it’s on the lab drive, connect the VPN and the drive. Browsing and marking still work; only Refresh and opening full-res files need it.">● Folder offline</span>` : '';
  if (typeof renderProjectBtn === 'function') renderProjectBtn();
  renderLogo();
  const all = S.data.slices, n = all.length;
  const count = st => all.filter(s => markOf(s.id).status === st).length;
  const go = v => `data-act="filter" data-g="mark" data-v="${v}" title="Show these"`;
  $('#stats').innerHTML = n ? `<span class="stat"><b>${new Set(all.map(s => s.slide)).size}</b>slides</span>
    <span class="stat"><b>${n}</b>images</span><span class="stat good click" ${go('good')}><b>${count('good')}</b>good</span>
    <span class="stat ok click" ${go('ok')}><b>${count('ok')}</b>okay</span>
    <span class="stat click" ${go('unmarked')}><b>${n - count('good') - count('ok') - count('bad')}</b>to review</span>` : '';
}

function chip(group, value, label, on, extra = '') {
  return `<button class="chip ${extra} ${on ? 'on' : ''}" data-act="filter" data-g="${group}" data-v="${esc(value)}">${label}</button>`;
}

function progressBar(ss) {
  const w = st => ss.filter(s => markOf(s.id).status === st).length / ss.length * 100;
  return `<span class="prog"><i style="width:${w('good')}%;background:var(--good)"></i><i style="width:${w('ok')}%;background:var(--ok)"></i><i style="width:${w('bad')}%;background:var(--bad)"></i></span>`;
}

function renderFilters() {
  const all = S.data.slices;
  const conds = [...new Set(all.map(s => s.condition))];
  const stains = [...new Set(all.flatMap(s => (s.channels || []).map(c => c.stain)))];
  const slidesN = [...new Set(all.map(s => s.slide))].sort((a, b) => a - b);
  const nOf = k => k === 'all' ? all.length : all.filter(s => { const m = markOf(s.id); return k === 'unmarked' ? !m.status : k === 'note' ? !!m.note : m.status === k; }).length;
  const seg = (k, l, wide) => `<button class="${S.f.mark === k ? 'on' : ''} ${wide ? 'wide' : ''}" data-act="filter" data-g="mark" data-v="${k}">${l}<span class="n">${nOf(k)}</span></button>`;
  $('#filters').innerHTML = `
    <h4>Show</h4>
    <div class="seg">${seg('all', 'All', true)}${seg('unmarked', 'To review')}${seg('good', '✓ Good')}${seg('ok', '~ Okay')}${seg('bad', '✗ Not good')}${seg('note', '✎ With notes', true)}</div>
    <h4>Condition</h4><div class="chips">${conds.map(c => chip('cond', c, esc(c), S.f.cond.has(c), 'cond-' + c)).join('')}</div>
    <h4>Stain</h4><div class="chips">${stains.map(c => chip('stain', c, dot(c) + esc(stainShort(c)), S.f.stain.has(c))).join('')}</div>
    <h4>Slide</h4><div class="slide-list">
      <button class="slide-btn ${S.f.slide == null ? 'on' : ''}" data-act="filter" data-g="slide" data-v="">
        <span class="num">All</span><span class="meta">Every slide</span><span class="count">${all.length}</span></button>
      ${slidesN.map(n => {
        const ss = all.filter(s => s.slide === n), s0 = ss[0], exp = +(S.data.settings.expectedSlices || {})[n];
        const warn = exp && exp !== ss.length ? ` <span class="warn" title="Your expected count (Settings) is ${exp}">≠${exp}</span>` : '';
        return `<button class="slide-btn ${S.f.slide === n ? 'on' : ''}" data-act="filter" data-g="slide" data-v="${n}">
          <span class="num">${n ? `SL${pad(n)}` : 'No #'}</span>
          <span class="meta">${esc([s0.block, s0.condition].filter(Boolean).join(' · '))}${progressBar(ss)}</span>
          <span class="count">${ss.length}${warn}</span></button>`;
      }).join('')}</div>
    <p class="note">The bar under each slide shows images marked <span style="color:var(--good)">good</span>, <span style="color:var(--ok)">okay</span> and <span style="color:var(--bad)">not good</span>.
      ${Object.keys(S.data.settings.expectedSlices || {}).length ? '<span style="color:var(--accent)">≠</span> means the number of images found differs from the expected count in Settings.' : ''}</p>`;
}

function renderGrid() {
  const list = filtered();
  if (!S.data.slices.length) {
    const busy = S.data.build?.phase && !['idle', 'done', 'error'].includes(S.data.build.phase);
    const noFolder = !S.data.settings.imageFolder;
    $('#grid').innerHTML = `<div class="empty"><div><div class="empty-fl">${flower(busy ? 0.6 : 0, { size: 64, bloom: true, busy })}</div>
      <div class="big">${busy ? 'Making your previews…' : noFolder ? 'Welcome to Fluora' : 'No previews yet'}</div>
      <div>${busy ? 'Copying a small preview of each image. Images appear here when it’s done.'
        : noFolder ? 'Start by telling it where your slide-scanner images are.'
        : 'Click <b>Refresh</b> to look for slides in your image folder. If it’s on the lab drive, connect the VPN first.'}</div>
      ${busy ? '' : noFolder ? '<button class="primary" style="margin-top:16px" data-act="setup-open">Choose image folder</button>'
        : '<button class="primary" style="margin-top:16px" data-act="rebuild">Refresh</button>'}</div></div>`;
    bloomFlowers($('#grid'));
    return;
  }
  if (!list.length) {
    $('#grid').innerHTML = '<div class="empty"><div><div class="big">Nothing matches</div><div>Try clearing a filter on the left.</div></div></div>';
    return;
  }
  const bySlide = new Map();
  list.forEach(s => (bySlide.get(s.slide) || bySlide.set(s.slide, []).get(s.slide)).push(s));
  $('#grid').innerHTML = [...bySlide].map(([n, ss]) => {
    const s0 = ss[0], whole = S.data.slices.filter(s => s.slide === n);
    const reviewed = whole.filter(s => markOf(s.id).status).length;
    return `<section class="slide-group"><div class="group-head">
        <h3>${slideName(n)}<span class="mono">${esc(s0.block)}</span></h3>
        ${s0.cond ? `<span class="tag cond-${esc(s0.condition)}">${esc(s0.condition)}</span>` : ''}
        ${(s0.channels || []).map(c => `<span class="tag">${dot(c.stain)}${esc(stainName(c.stain))}</span>`).join('')}
        <span class="reviewed">${reviewed}/${whole.length} reviewed</span></div>
      <div class="tiles ${S.f.mark === 'note' ? 'with-notes' : ''}">${ss.map(s => {
        const m = markOf(s.id);
        const tile = `<div class="tile ${m.status || ''}" data-act="view" data-id="${s.id}" ${m.note ? `title="${esc(m.note)}"` : ''}>
          <img loading="lazy" src="${imgUrl(s.id, 'thumb')}" alt="${slideName(s.slide)} image ${s.im}">
          <div class="corner r">${typeof pickBadge === 'function' ? pickBadge(m.pick) : ''}${typeof snapBadge === 'function' ? snapBadge(s.id) : ''}${m.note ? '<span title="Has a note">✎</span>' : ''}</div>
          <div class="cap"><span class="mono">IM${pad(s.im)}</span>
            ${m.status === 'good' ? '<span class="pill good">✓ Good</span>' : m.status === 'ok' ? '<span class="pill ok">~ Okay</span>' : m.status === 'bad' ? '<span class="pill bad">✗ Not good</span>' : ''}</div>
        </div>`;
        return S.f.mark === 'note' ? `<div class="note-item">${tile}<p data-act="view" data-id="${s.id}">${esc(m.note)}</p></div>` : tile;
      }).join('')}</div></section>`;
  }).join('');
}

function render() {
  renderBuild(); renderFilters(); renderGrid();
  if (S.tab === 'board' && typeof renderBoard === 'function') renderBoard();
  if (S.tab === 'gallery' && typeof renderGallery === 'function') renderGallery();
  if (S.tab === 'snaps' && typeof renderSnaps === 'function') renderSnaps();
  if (S.tab === 'bouquet' && typeof renderBouquet === 'function') renderBouquet();
  if (S.v.open) renderViewer();
}

// ---------------------------------------------------------------- viewer

function current() { return S.data.slices.find(s => s.id === S.v.id); }

function openViewer(id, list = filtered().map(s => s.id)) {
  // Freeze the order the viewer steps through, so an image marked under "To review" doesn't drop out from under you
  Object.assign(S.v, { open: true, id, scale: 0, list });
  $('#viewer').hidden = false;
  renderViewer();
  if (typeof maybeViewerTips === 'function') maybeViewerTips();
}
function closeViewer() {
  flushNote(); S.v.open = false; $('#viewer').hidden = true; renderBuild(); renderFilters(); renderGrid();
  if (S.tab === 'board' && typeof renderBoard === 'function') renderBoard();
  if (S.tab === 'gallery' && typeof renderGallery === 'function') renderGallery();
  if (S.tab === 'snaps' && typeof renderSnaps === 'function') renderSnaps();
}

function step(d) {
  flushNote();
  const list = S.v.list, i = list.indexOf(S.v.id);
  const next = list[(i + d + list.length) % list.length];
  if (next) { S.v.id = next; S.v.scale = 0; renderViewer(); }
}

// G / O / X: mark this image and go straight to the next one (stays put on the last image)
function markAndNext(status) {
  setMark({ status }).catch(e => alert(`Couldn't save the mark: ${e.message}`));
  if (S.v.list.indexOf(S.v.id) < S.v.list.length - 1) step(1); else renderViewer();
}

// µm per preview pixel: the full-resolution pixel size times how much smaller the preview is
const fullSizeOf = s => (s.fullSize?.[0] ? s.fullSize : (s.previewSize || [0, 0]).map(v => v * 2 ** ((s.previewLevel || 1) - 1)));
const viewUm = s => s.umPerPx && s.previewSize?.[0] ? s.umPerPx * fullSizeOf(s)[0] / s.previewSize[0] : null;  // TIFFs store the full-res pixel size at every level

function renderViewer() {
  const s = current();
  if (!s) return closeViewer();
  const m = markOf(s.id);
  const list = S.v.list, pos = list.indexOf(s.id);
  if (S.v.channel !== 'merged' && !(s.channels || []).some(c => c.stain === S.v.channel)) S.v.channel = 'merged';
  const [pw, ph] = s.previewSize || [0, 0];
  const full = fullSizeOf(s);
  const mb = Math.round(full[0] * full[1] * (s.channels || []).length * 2 / 1e6);
  $('#viewer').innerHTML = `
    <div class="stage" id="stage">
      <img id="vimg" src="${imgUrl(s.id, S.v.channel === 'merged' ? 'merged' : S.v.channel)}" style="filter:brightness(${S.v.bright})" draggable="false" alt="">
      <div class="topbar"><div class="title glass"><b>${slideName(s.slide)}</b> · IM${pad(s.im)} <span class="muted">· ${pos + 1} of ${list.length}</span></div></div>
      <button class="nav prev glass" data-act="step" data-d="-1" title="Previous (←)">‹</button>
      <button class="nav next glass" data-act="step" data-d="1" title="Next (→)">›</button>
      <button class="snapbtn glass" data-act="snap" title="Snap what you see, with a scale bar (S)">${CAMERA_SVG}<span>Snap</span><kbd>S</kbd></button>
      <div class="zoom glass"><button data-act="zoom" data-k="1.5" title="Zoom in">+</button><button data-act="zoom" data-k="0.667" title="Zoom out">−</button><button data-act="fit" title="Fit (double-click)">⤢</button></div>
      <div class="scalebar glass" id="scalebar"></div>
      <div class="hint glass"><span><kbd>←</kbd> <kbd>→</kbd> next</span><span><kbd>G</kbd> <kbd>O</kbd> <kbd>X</kbd> mark &amp; next</span><span><kbd>1</kbd>–<kbd>3</kbd> channels</span><span><kbd>S</kbd> snap</span><span><kbd>B</kbd> board</span><span><kbd>P</kbd> pick</span><span><kbd>F</kbd> Fiji</span><span><kbd>⇧F</kbd> channels</span><span><kbd>Esc</kbd> close</span></div>
    </div>
    <div class="side">
      <div class="side-head"><div><h2>${slideName(s.slide)} <span class="mono">IM${pad(s.im)}</span></h2>
        <div class="sub">${s.cond ? `<span class="tag cond-${esc(s.condition)}">${esc(s.condition)}</span>` : ''}${s.block ? `<span class="tag">Block ${esc(s.block)}</span>` : ''}</div></div>
        <button class="ghost" data-act="close" title="Close (Esc)">✕</button></div>
      <section><div class="label">Mark this image${m.status ? '<button class="link" data-act="mark" data-s="">Clear mark</button>' : ''}</div>
        ${m.by && (m.status || m.note) ? `<p class="by">Last changed by <b>${esc(m.by)}</b>${m.updated ? ` · ${new Date(m.updated * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' })}` : ''}</p>` : ''}
        <div class="marks">
          <button class="good ${m.status === 'good' ? 'on' : ''}" data-act="mark" data-s="good">✓ Good <kbd>G</kbd></button>
          <button class="ok ${m.status === 'ok' ? 'on' : ''}" data-act="mark" data-s="ok">~ Okay <kbd>O</kbd></button>
          <button class="bad ${m.status === 'bad' ? 'on' : ''}" data-act="mark" data-s="bad">✗ Not good <kbd>X</kbd></button></div>
        ${typeof pickBtn === 'function' ? pickBtn(imagePicked(s.id), 'pick') : ''}
        <textarea placeholder="Notes about this image…" data-in="note">${esc(m.note || '')}</textarea>
        <div class="note-status" id="note-status" aria-live="polite">Marks and notes save by themselves</div>
        <button class="board-add ${onBoard(s.id) ? 'on' : ''}" data-act="b-add">${onBoard(s.id) ? '✓ On the board · add again' : '＋ Add to board'}<kbd class="k">B</kbd></button></section>
      ${typeof viewerSnapsHtml === 'function' ? viewerSnapsHtml(s) : ''}
      <section><div class="label">Channel</div><div class="channels">
        <button class="${S.v.channel === 'merged' ? 'on' : ''}" data-act="channel" data-c="merged">
          <span style="display:flex;gap:2px">${(s.channels || []).map(c => dot(c.stain)).join('')}</span>Merged<kbd class="k">M</kbd></button>
        ${(s.channels || []).map((c, i) => `<button class="${S.v.channel === c.stain ? 'on' : ''}" data-act="channel" data-c="${c.stain}">
          ${dot(c.stain)}${esc(stainName(c.stain))}<kbd class="k">${i + 1}</kbd></button>`).join('')}
      </div></section>
      <section><div class="label"><span>Screen brightness</span><span class="mono">${S.v.bright.toFixed(1)}×</span></div>
        <input type="range" min="0.5" max="4" step="0.1" value="${S.v.bright}" data-in="bright">
        <p class="fine">Only changes what you see here. To change the saved images for a whole stain, use Brightness &amp; contrast.</p></section>
      <section><div class="label">Details</div><dl>
        <dt>Stains</dt><dd>${esc(s.stains.join(', '))}</dd>
        <dt>Full res</dt><dd>~${full[0].toLocaleString()} × ${full[1].toLocaleString()} px</dd>
        ${s.umPerPx ? `<dt>Pixel size</dt><dd>${s.umPerPx.toFixed(3)} µm (full res)</dd>` : ''}
        <dt>Preview</dt><dd>${pw} × ${ph} px${s.previewLevel ? ` (R${s.previewLevel})` : ''}</dd>
        <dt>File</dt><dd class="mono" style="font-size:12px;word-break:break-all">${esc((s.levels['1'] || '').split(/[\\/]/).pop())}</dd></dl></section>
      <section><div class="label">Full resolution</div><div class="open">
        <button data-act="open" data-w="fiji">Open full-res merged in Fiji <kbd class="k" style="margin-left:auto">F</kbd></button>
        <button data-act="open" data-w="fiji-channels">Open full-res channels in Fiji <kbd class="k" style="margin-left:auto">⇧F</kbd></button>
        <button data-act="open" data-w="finder">${IS_WIN ? 'Show in File Explorer' : 'Show in Finder'}</button>
        ${s.czi ? '<button data-act="open" data-w="qupath">Open slide CZI in QuPath</button>' : ''}</div>
        <p class="fine">Opens the original file (on the lab drive this needs the VPN). It’s about ${mb.toLocaleString()} MB, so Fiji takes a minute or two.</p></section>
    </div>`;
  const img = $('#vimg');
  img.onload = () => { if (S.v.region && typeof zoomToRegion === 'function') zoomToRegion(); else if (!S.v.scale) fit(); else applyTransform(); };
  if (img.complete && img.naturalWidth) img.onload();
  bindStage();
}

function fit() {
  const st = $('#stage'), img = $('#vimg');
  if (!st || !img?.naturalWidth) return;
  const sc = Math.min(st.clientWidth / img.naturalWidth, st.clientHeight / img.naturalHeight) * 0.94;
  Object.assign(S.v, { scale: sc, x: (st.clientWidth - img.naturalWidth * sc) / 2, y: (st.clientHeight - img.naturalHeight * sc) / 2 });
  applyTransform();
}
function zoomAt(k, mx, my) {
  const ns = Math.min(Math.max(S.v.scale * k, 0.05), 12);
  S.v.x = mx - (mx - S.v.x) * ns / S.v.scale; S.v.y = my - (my - S.v.y) * ns / S.v.scale; S.v.scale = ns;
  applyTransform();
}
function applyTransform() {
  const img = $('#vimg');
  if (img) img.style.transform = `translate(${S.v.x}px,${S.v.y}px) scale(${S.v.scale})`;
  const bar = $('#scalebar'), s = current(), um = s && viewUm(s);
  if (!bar) return;
  if (!um) { bar.hidden = true; return; }
  const umPerScreenPx = um / S.v.scale;
  const nice = [10, 20, 50, 100, 200, 500, 1000, 2000, 5000].find(v => v / umPerScreenPx >= 70) || 5000;
  bar.hidden = false;
  bar.innerHTML = `<i style="width:${Math.round(nice / umPerScreenPx)}px"></i>${nice >= 1000 ? nice / 1000 + ' mm' : nice + ' µm'}`;
}
function bindStage() {
  const st = $('#stage');
  st.onwheel = e => {
    e.preventDefault();
    const r = st.getBoundingClientRect();
    zoomAt(Math.exp(-e.deltaY * 0.0015), e.clientX - r.left, e.clientY - r.top);
  };
  st.ondblclick = e => { if (!e.target.closest('button')) fit(); };
  st.onmousedown = e => {
    if (e.target.closest('button')) return;
    const sx = e.clientX - S.v.x, sy = e.clientY - S.v.y;
    st.classList.add('drag');
    const move = ev => { S.v.x = ev.clientX - sx; S.v.y = ev.clientY - sy; applyTransform(); };
    const up = () => { st.classList.remove('drag'); removeEventListener('mousemove', move); removeEventListener('mouseup', up); };
    addEventListener('mousemove', move); addEventListener('mouseup', up);
  };
}

async function setMark(fields, id = S.v.id) {
  S.data.marks[id] = { ...markOf(id), ...fields };  // show it straight away; the server reply confirms it
  const r = await api('/api/marks', { id, ...fields });
  S.data.marks = r.marks;
}

// ---------------------------------------------------------------- dialogs

function showModal(html) { $('#modal').innerHTML = `<div class="dialog">${html}</div>`; $('#modal').hidden = false; }
function closeModal() { if (!S.modalLocked) $('#modal').hidden = true; }

function contrastDialog() {
  const c = S.data.contrast;
  const stains = Object.keys(c).sort();
  showModal(`<h2>Brightness &amp; contrast</h2>
    <p>One setting per stain, used for every image with that stain, so conditions can be compared fairly by eye.</p>
    <p class="muted small">Values are raw 16-bit intensities. “Auto” values are measured across all images together. Applying remakes the preview images (about a minute). The original data is never changed.</p>
    <table><tr><th>Stain</th><th>Black at</th><th>White at</th><th>Gamma</th><th>In merge</th><th></th></tr>
    ${stains.map(st => `<tr data-stain="${st}"><td>${dot(st)}${esc(stainName(st))}</td>
      <td><input type="number" data-k="min" value="${Math.round(c[st].min)}"></td>
      <td><input type="number" data-k="max" value="${Math.round(c[st].max)}"></td>
      <td><input type="number" step="0.1" min="0.2" max="5" data-k="gamma" value="${c[st].gamma ?? 1}"></td>
      <td><input type="checkbox" data-k="show" ${c[st].show !== false ? 'checked' : ''}></td>
      <td class="small muted">${c[st].auto ? 'auto' : `<button class="ghost" data-act="contrast-auto" data-stain="${st}">Back to auto</button>`}</td></tr>`).join('')}</table>
    <div class="foot"><button class="ghost" data-act="close-modal">Cancel</button><button class="primary" data-act="contrast-save">Apply</button></div>`);
}

function readContrastForm() {
  const out = {};
  document.querySelectorAll('#modal tr[data-stain]').forEach(tr => {
    const g = k => tr.querySelector(`[data-k="${k}"]`);
    out[tr.dataset.stain] = { min: +g('min').value, max: +g('max').value, gamma: +g('gamma').value || 1, show: g('show').checked, auto: false };
  });
  return out;
}

function goodList() {
  const bySlide = {};
  S.data.slices.forEach(s => { if (markOf(s.id).status === 'good') (bySlide[s.slide] ||= []).push(s.im); });
  const lines = Object.keys(bySlide).sort((a, b) => a - b).map(n => `${slideName(n)}: ${bySlide[n].sort((a, b) => a - b).join(', ')}`);
  showModal(`<h2>Good snaps by slide</h2>
    <p>Image (series) numbers marked good, slide by slide, ready to paste into your tracking spreadsheet.</p>
    <pre id="good-text">${esc(lines.join('\n') || 'No images marked good yet.')}</pre>
    <div class="foot"><button class="ghost" data-act="close-modal">Close</button><button class="primary" data-act="copy-good">Copy</button></div>`);
}

// Settings: where the images are, plus the names and counts that are specific to your project
const pairs = (obj) => Object.entries(obj || {}).map(([k, v]) => `${k} = ${v}`).join('\n');
function readPairs(text, numeric) {
  const out = {};
  text.split('\n').forEach(line => {
    const m = line.match(/^\s*([^=:]+?)\s*[=:]\s*(.+?)\s*$/);
    if (m && (!numeric || /^\d+$/.test(m[2]))) out[m[1]] = numeric ? +m[2] : m[2];
  });
  return out;
}
function settingsDialog() {
  const st = S.data.settings || {}, apps = S.data.apps || {};
  const field = (k, label, help, ph = '') => `<label class="field"><span>${label}</span>
    <input type="text" data-k="${k}" value="${esc(st[k] || '')}" placeholder="${esc(ph)}" spellcheck="false">${help ? `<small>${help}</small>` : ''}</label>`;
  showModal(`<h2>Settings</h2>
    <p>For this project. Saved on this computer only.</p>
    <div class="form">
    <div class="field"><span>Images</span>
      <div class="pick"><span class="mono folder-now" title="${esc(st.imageFolder || '')}">${esc(st.imageFolder || 'No folder chosen yet')}</span>
        <button type="button" data-act="setup-open">Folder, labels &amp; channels…</button></div>
      <small>Where this project’s images are, how each one is labelled, and the name and colour of each stain.</small></div>
    ${field('projectName', 'Project name', 'Shown under the app’s name. Optional.', 'e.g. Paraffin sections, 2025')}
    <label class="field"><span>Condition codes</span>
      <textarea data-k="conditions" rows="4" placeholder="C = Control&#10;E = Exposed" spellcheck="false">${esc(pairs(st.conditions))}</textarea>
      <small>The second part of a file name like <span class="mono">01q_<b>E</b>_SL04_IM01…</span>, one per line. Optional: without them the code itself is shown.</small></label>
    <label class="field"><span>Expected images per slide</span>
      <textarea data-k="expectedSlices" rows="3" placeholder="1 = 14&#10;2 = 18" spellcheck="false">${esc(pairs(st.expectedSlices))}</textarea>
      <small>From your tracking spreadsheet, slide number = number of images. A slide whose count differs gets a ≠ in the slide list. Optional.</small></label>
    ${field('fiji', 'Fiji', apps.fiji && !st.fiji ? `Found automatically: <span class="mono">${esc(apps.fiji)}</span>` : 'Leave blank to find it automatically.', apps.fiji || 'Not found. Paste where Fiji is installed')}
    ${field('qupath', 'QuPath', apps.qupath && !st.qupath ? `Found automatically: <span class="mono">${esc(apps.qupath)}</span>` : 'Leave blank to find it automatically.', apps.qupath || 'Not found. Paste where QuPath is installed')}
    </div>
    <div class="foot"><button class="ghost" data-act="close-modal">Cancel</button><button class="primary" data-act="settings-save">Save</button></div>`);
}
async function saveSettings() {
  const g = k => $(`#modal [data-k="${k}"]`).value.trim();
  const before = S.data.settings.imageFolder;
  const body = { projectName: g('projectName'), fiji: g('fiji'), qupath: g('qupath'),
    conditions: readPairs(g('conditions')), expectedSlices: readPairs(g('expectedSlices'), true) };
  const r = await api('/api/settings', body);
  closeModal();
  await load();
  void r; void before;
}

// ---------------------------------------------------------------- events

const ACT = {
  filter: el => {
    const { g, v } = el.dataset;
    if (g === 'mark') S.f.mark = v;
    else if (g === 'slide') S.f.slide = v === '' ? null : +v;
    else S.f[g].has(v) ? S.f[g].delete(v) : S.f[g].add(v);
    renderFilters(); renderGrid();
  },
  view: el => openViewer(el.dataset.id),
  close: closeViewer,
  step: el => step(+el.dataset.d),
  zoom: el => { const st = $('#stage'); zoomAt(+el.dataset.k, st.clientWidth / 2, st.clientHeight / 2); },
  fit,
  channel: el => { S.v.channel = el.dataset.c; renderViewer(); },
  mark: el => {
    setMark({ status: el.dataset.s || null }).then(() => noteStatus('Mark saved', 'ok')).catch(e => alert(`Couldn't save the mark: ${e.message}`));
    renderViewer();
  },
  open: el => api('/api/open', { id: S.v.id, what: el.dataset.w }).catch(e => alert(e.message)),
  rebuild: () => (S.data.hasLabels ? openSetup({ refresh: true }) : S.data.settings.imageFolder ? confirm('Look for new or changed images in your image folder?\n\nIf it’s on the lab drive this needs the VPN, and it can take a while. Your marks and notes are kept.')
    && api('/api/rebuild', {}).then(poll) : settingsDialog()),
  settings: settingsDialog,
  'setup-open': () => { closeModal(); openSetup({ autoDetect: true }); },
  'pick-folder': async () => {
    const input = $('#modal [data-k="imageFolder"]');
    const r = await api('/api/pick-folder', { start: input.value.trim() });
    if (r.path) input.value = r.path;
  },
  'settings-save': () => saveSettings().catch(e => alert(`Couldn't save the settings: ${e.message}`)),
  contrast: contrastDialog,
  'contrast-save': () => api('/api/contrast', { contrast: readContrastForm() }).then(() => { closeModal(); poll(); }),
  'contrast-auto': el => api('/api/contrast', { contrast: { [el.dataset.stain]: { auto: true } } }).then(() => { closeModal(); poll(); }),
  'good-list': goodList,
  'copy-good': el => navigator.clipboard.writeText($('#good-text').textContent).then(() => { el.textContent = 'Copied ✓'; }),
  'close-modal': closeModal,
  tab: el => showTab(el.dataset.t),
};

const TAB_VIEWS = { slides: '#slides-view', gallery: '#gallery', board: '#board', snaps: '#snaps', bouquet: '#bouquet' };
function showTab(t) {
  S.tab = TAB_VIEWS[t] ? t : 'slides';
  try { localStorage.setItem('slides-tab', S.tab); } catch {}
  Object.entries(TAB_VIEWS).forEach(([k, sel]) => { $(sel).hidden = k !== S.tab; });
  document.querySelectorAll('.tabs button').forEach(b => b.classList.toggle('on', b.dataset.t === S.tab));
  if (S.tab === 'board') renderBoard();
  if (S.tab === 'gallery') renderGallery();
  if (S.tab === 'snaps') renderSnaps();
  if (S.tab === 'bouquet') renderBouquet();
}

document.addEventListener('click', e => {
  const el = e.target.closest('[data-act]');
  if (el) { e.preventDefault(); ACT[el.dataset.act]?.(el); }
  else if (e.target.id === 'modal') closeModal();
});
let noteTimer, pendingNote = null;
// Shows under the notes box that a note (or mark) is saving or saved
function noteStatus(text, kind = '') {
  const el = $('#note-status');
  if (!el) return;
  el.textContent = text;
  el.className = `note-status ${kind}`;
}
// Notes save half a second after you stop typing, always to the image they were typed on
function flushNote() {
  clearTimeout(noteTimer);
  if (pendingNote) {
    const { id, note } = pendingNote; pendingNote = null;
    setMark({ note }, id).then(() => noteStatus('Saved', 'ok')).catch(e => { noteStatus('Not saved', 'bad'); alert(`Couldn't save the note: ${e.message}`); });
  }
}
document.addEventListener('input', e => {
  const k = e.target.dataset.in;
  if (k === 'bright') {
    S.v.bright = +e.target.value;
    $('#vimg').style.filter = `brightness(${S.v.bright})`;
    e.target.closest('section').querySelector('.label .mono').textContent = S.v.bright.toFixed(1) + '×';
  }
  if (k === 'note') { clearTimeout(noteTimer); pendingNote = { id: S.v.id, note: e.target.value }; noteStatus('Saving…'); noteTimer = setTimeout(flushNote, 500); }
});
document.addEventListener('keydown', e => {
  if (e.key === 'Escape' && !$('#modal').hidden) { closeModal(); return; }
  if (!S.v.open || e.target.matches('textarea, input')) return;
  const s = current(), k = e.key.toLowerCase();
  if (e.key === 'ArrowRight') step(1);
  else if (e.key === 'ArrowLeft') step(-1);
  else if (e.key === 'Escape') closeViewer();
  else if (k === 'g') markAndNext('good');
  else if (k === 'o') markAndNext('ok');
  else if (k === 'x') markAndNext('bad');
  else if (k === 'b') ACT['b-add']();
  else if (k === 'p') ACT.pick();
  else if (k === 's' && !e.metaKey && !e.ctrlKey) ACT.snap();
  else if (k === 'f') ACT.open({ dataset: { w: e.shiftKey ? 'fiji-channels' : 'fiji' } });
  else if (k === 'm' || k === '0') { S.v.channel = 'merged'; renderViewer(); }
  else if (/^[1-9]$/.test(k) && s.channels?.[+k - 1]) { S.v.channel = s.channels[+k - 1].stain; renderViewer(); }
  else return;
  e.preventDefault();
});
addEventListener('resize', () => S.v.open && fit());

// ---------------------------------------------------------------- loading

let lastBuilt = null, pollTimer;
async function load() {
  S.data = await api('/api/slides');
  lastBuilt = S.data.builtAt;
  render();
  if (typeof maybeStartTour === 'function') maybeStartTour();
}
async function poll() {
  clearTimeout(pollTimer);
  try {
    const st = await api('/api/status');
    S.data.build = st.build; S.data.labConnected = st.labConnected;
    if (st.builtAt !== lastBuilt) await load(); else renderBuild();
    const busy = st.build.phase && !['idle', 'done', 'error'].includes(st.build.phase);
    pollTimer = setTimeout(poll, busy ? 1500 : 15000);
  } catch { pollTimer = setTimeout(poll, 5000); }
}
load().then(() => { if (typeof firstRun === 'function') firstRun(); let t = null; try { t = localStorage.getItem('slides-tab'); } catch {} if (t && t !== 'slides') showTab(t); }).then(poll);

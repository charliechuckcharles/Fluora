/* Board: drag images onto a big canvas and sort them into named groups, like a mind map.
   Saved in data/board.json on this computer. An image can be on the board more than once. An image
   belongs to every group its centre sits inside, so moving a group carries its images with it. */
'use strict';

const BOARD = { groups: [], cards: [], view: { x: 40, y: 40, k: 1 } };
const TRAY = { show: 'good', q: '' };
const CARD_W = 184, CARD_H = 204;
const GROUP_COLORS = ['#f4d77a', '#5cc8d6', '#46c98b', '#f0a259', '#c58cff', '#ef6b6b'];
let boardLoaded = false, boardSaveTimer;

const uid = p => p + Math.random().toString(36).slice(2, 9);
const sliceById = id => S.data.slices.find(s => s.id === id);
const sliceLabel = s => `${slideName(s.slide)} · IM${pad(s.im)}`;
const sliceMeta = s => [s.block, s.condition].filter(Boolean).join(' · ');
const onBoard = id => BOARD.cards.some(c => c.slice === id);
const pillFor = st => ({ good: '<span class="pill good">✓ Good</span>', ok: '<span class="pill ok">~ Okay</span>',
  bad: '<span class="pill bad">✗ Not good</span>' }[st] || '');
const inGroup = (c, g) => {
  const cx = c.x + CARD_W / 2, cy = c.y + CARD_H / 2;
  return cx > g.x && cx < g.x + g.w && cy > g.y && cy < g.y + g.h;
};
const groupCards = g => BOARD.cards.filter(c => inGroup(c, g)).sort((a, b) => a.y - b.y || a.x - b.x);

async function loadBoard() {
  try { Object.assign(BOARD, await api('/api/board')); } catch (e) { console.warn('Board not loaded', e); }
  BOARD.view = { x: 40, y: 40, k: 1, ...BOARD.view };
  boardLoaded = true;
  if (S.tab === 'board') renderBoard();
}
function saveBoard() {
  clearTimeout(boardSaveTimer);
  boardSaveTimer = setTimeout(() => api('/api/board', BOARD).catch(e => alert(`Couldn't save the board: ${e.message}`)), 400);
}

// ---------------------------------------------------------------- drawing

function renderBoard() {
  const el = $('#board');
  if (!boardLoaded) { el.innerHTML = '<div class="empty"><div>Loading the board…</div></div>'; return; }
  const empty = !BOARD.cards.length && !BOARD.groups.length;
  el.innerHTML = `<aside class="tray">${trayHtml()}</aside>
    <div class="bcanvas" id="bcanvas">
      <div class="bworld" id="bworld">${drawWorld()}</div>
      <div class="btools glass">
        <button data-act="b-group" title="Make a new group (or double-click empty space)">＋ Group</button>
        <button data-act="b-fit" title="Show everything on the board">Fit</button>
        <button data-act="b-zoom" data-k="1.25" title="Zoom in">+</button>
        <button data-act="b-zoom" data-k="0.8" title="Zoom out">−</button></div>
      ${empty ? `<div class="bempty"><div class="big">Your board is empty</div>
        <div>Drag images from the list on the left onto here, then make groups for what they’re good for,
        e.g. “Fungal bodies” or “Figure 2”. Each group’s <b>Copy list</b> gives its slide and image numbers.</div></div>` : ''}
      <div class="bhint glass">Saves by itself · Drag empty space to move around · Scroll to zoom · Double-click for a new group · Click an image to open it</div>
    </div>`;
  applyBoardView();
  bindBoard();
}

function drawWorld() {
  const groups = [...BOARD.groups].sort((a, b) => b.w * b.h - a.w * a.h);  // big ones behind small ones
  return groups.map(groupHtml).join('') + BOARD.cards.map(cardHtml).join('');
}
function redrawWorld() { const w = $('#bworld'); if (w) w.innerHTML = drawWorld(); }

function groupHtml(g) {
  const n = groupCards(g).length;
  return `<div class="bgroup" data-group="${g.id}" style="left:${g.x}px;top:${g.y}px;width:${g.w}px;height:${g.h}px;--gc:${g.color}">
    <div class="bghead" title="Drag to move this group and its images">
      <span class="bgrab" aria-hidden="true">⠿</span>
      <input value="${esc(g.title)}" size="${Math.max(14, g.title.length + 1)}" data-gtitle="${g.id}" placeholder="Name this group" spellcheck="false">
      <span class="bgfill"></span>
      <span class="bgn">${n} image${n === 1 ? '' : 's'}</span>
      <button class="ghost" data-act="b-copy" data-id="${g.id}" title="Copy the slide and image numbers in this group">Copy list</button>
      <button class="ghost dotbtn" data-act="b-color" data-id="${g.id}" title="Change colour"><i></i></button>
      <button class="ghost" data-act="b-gdel" data-id="${g.id}" title="Remove this group (its images stay on the board)">✕</button></div>
    <div class="bgrip" data-grip="${g.id}" title="Drag to resize"></div></div>`;
}

function cardHtml(c) {
  const del = `<button class="bx" data-act="b-del" data-id="${c.id}" title="Take off the board">✕</button>`;
  if (c.snap) return snapCardHtml(c, del);
  const s = sliceById(c.slice);
  if (!s) return `<div class="bcard missing" data-card="${c.id}" style="left:${c.x}px;top:${c.y}px">
    <div class="bimg">${esc(c.slice)} isn’t in the slide list any more</div>${del}</div>`;
  const m = markOf(s.id);
  return `<div class="bcard ${m.status || ''}" data-card="${c.id}" style="left:${c.x}px;top:${c.y}px" title="${esc(m.note || '')}">
    <div class="bimg"><img src="${imgUrl(s.id, 'thumb')}" alt="" draggable="false"></div>
    <div class="bcap"><b>${sliceLabel(s)}</b>${pillFor(m.status)}</div>
    <div class="bsub">${esc(sliceMeta(s))}${m.note ? ' · ✎ note' : ''}</div>${del}</div>`;
}

function trayHtml() {
  const seg = (k, l) => `<button class="${TRAY.show === k ? 'on' : ''}" data-act="b-tray" data-v="${k}">${l}</button>`;
  return `<h4>Drag onto the board</h4>
    <div class="seg">${seg('good', '✓ Good')}${seg('ok', '~ Okay')}${seg('note', '✎ Notes')}${seg('snaps', 'Snaps')}${seg('all', 'All')}</div>
    <input type="search" class="tray-q" data-in="tray-q" placeholder="Slide number, e.g. 22" value="${esc(TRAY.q)}">
    <div class="tray-list" id="tray-list">${trayList()}</div>`;
}
function trayList() {
  const q = TRAY.q.trim().replace(/^sl\s*/i, '').replace(/^0+(?=\d)/, '');
  if (TRAY.show === 'snaps') return snapTrayList(q);
  const list = S.data.slices.filter(s => {
    const m = markOf(s.id);
    return (TRAY.show === 'all' || (TRAY.show === 'note' ? !!m.note : m.status === TRAY.show)) && (!q || String(s.slide) === q);
  });
  if (!list.length) return `<p class="note">${TRAY.show === 'all' ? 'No images match.' : `No ${{ good: 'good', ok: 'okay', note: 'noted' }[TRAY.show]} images${q ? ' on that slide' : ''} yet.`}</p>`;
  return list.map(s => {
    const m = markOf(s.id);
    return `<div class="tray-item" draggable="true" data-slice="${s.id}" title="${esc(m.note || 'Drag onto the board')}">
      <img src="${imgUrl(s.id, 'thumb')}" alt="" loading="lazy" draggable="false">
      <div><b>${sliceLabel(s)}</b><span>${esc(sliceMeta(s))}</span>
      ${m.note ? `<span class="tn">✎ ${esc(m.note)}</span>` : ''}${onBoard(s.id) ? '<span class="onb">On the board</span>' : ''}</div></div>`;
  }).join('');
}

function applyBoardView() {
  const w = $('#bworld'), v = BOARD.view;
  if (w) w.style.transform = `translate(${v.x}px,${v.y}px) scale(${v.k})`;
  const c = $('#bcanvas');
  if (c) c.style.backgroundPosition = `${v.x}px ${v.y}px`, c.style.backgroundSize = `${24 * v.k}px ${24 * v.k}px`;
}

// ---------------------------------------------------------------- moving things around

function toWorld(ev) {
  const r = $('#bcanvas').getBoundingClientRect(), v = BOARD.view;
  return { x: (ev.clientX - r.left - v.x) / v.k, y: (ev.clientY - r.top - v.y) / v.k };
}
function zoomBoard(f, cx, cy) {
  const v = BOARD.view, k = Math.min(Math.max(v.k * f, 0.15), 2.5);
  v.x = cx - (cx - v.x) * k / v.k; v.y = cy - (cy - v.y) * k / v.k; v.k = k;
  applyBoardView(); saveBoard();
}
function newGroup(x, y) {
  const g = { id: uid('g'), title: '', x: Math.round(x), y: Math.round(y), w: 560, h: 320,
    color: GROUP_COLORS[BOARD.groups.length % GROUP_COLORS.length] };
  BOARD.groups.push(g); saveBoard(); renderBoard();
  $(`[data-gtitle="${g.id}"]`)?.focus();
}
function openFromBoard(card) {
  if (card.snap) { openSnap(card.snap); return; }
  // Step through the images in this card's (smallest) group, or the whole board
  const g = BOARD.groups.filter(g => inGroup(card, g)).sort((a, b) => a.w * a.h - b.w * b.h)[0];
  const list = [...new Set((g ? groupCards(g) : [...BOARD.cards].sort((a, b) => a.y - b.y || a.x - b.x)).filter(c => c.slice).map(c => c.slice))]
    .filter(id => sliceById(id));
  if (sliceById(card.slice)) openViewer(card.slice, list);
}

function bindBoard() {
  const cv = $('#bcanvas');
  cv.onwheel = e => { e.preventDefault(); const r = cv.getBoundingClientRect(); zoomBoard(Math.exp(-e.deltaY * 0.0015), e.clientX - r.left, e.clientY - r.top); };
  cv.ondblclick = e => {
    if (e.target.closest('.bcard, .bghead, .btools, button, input')) return;
    const p = toWorld(e); newGroup(p.x - 40, p.y - 20);
  };
  cv.onmousedown = e => {
    if (e.button !== 0 || e.target.closest('button, input, .btools')) return;
    e.preventDefault();
    const start = { x: e.clientX, y: e.clientY }, k = BOARD.view.k;
    let moved = false, onMove, onUp = () => {};
    const grip = e.target.closest('[data-grip]'), head = e.target.closest('.bghead'), cardEl = e.target.closest('.bcard');
    if (grip) {
      const g = BOARD.groups.find(g => g.id === grip.dataset.grip), w0 = g.w, h0 = g.h, el = grip.parentElement;
      onMove = (dx, dy) => { g.w = Math.max(220, Math.round(w0 + dx / k)); g.h = Math.max(140, Math.round(h0 + dy / k)); el.style.width = g.w + 'px'; el.style.height = g.h + 'px'; };
      onUp = () => redrawWorld();
    } else if (head) {
      const g = BOARD.groups.find(g => g.id === head.parentElement.dataset.group);
      const members = groupCards(g).map(c => ({ c, x: c.x, y: c.y, el: $(`[data-card="${c.id}"]`) }));
      const inner = BOARD.groups.filter(o => o !== g && o.x >= g.x && o.y >= g.y && o.x + o.w <= g.x + g.w && o.y + o.h <= g.y + g.h)
        .map(o => ({ o, x: o.x, y: o.y, el: $(`[data-group="${o.id}"]`) }));
      const gx = g.x, gy = g.y, el = head.parentElement;
      onMove = (dx, dy) => {
        g.x = Math.round(gx + dx / k); g.y = Math.round(gy + dy / k); el.style.left = g.x + 'px'; el.style.top = g.y + 'px';
        members.forEach(m => { m.c.x = Math.round(m.x + dx / k); m.c.y = Math.round(m.y + dy / k); m.el.style.left = m.c.x + 'px'; m.el.style.top = m.c.y + 'px'; });
        inner.forEach(m => { m.o.x = Math.round(m.x + dx / k); m.o.y = Math.round(m.y + dy / k); m.el.style.left = m.o.x + 'px'; m.el.style.top = m.o.y + 'px'; });
      };
      onUp = () => redrawWorld();
    } else if (cardEl) {
      const c = BOARD.cards.find(c => c.id === cardEl.dataset.card), x0 = c.x, y0 = c.y;
      BOARD.cards.push(BOARD.cards.splice(BOARD.cards.indexOf(c), 1)[0]);  // bring to front
      $('#bworld').appendChild(cardEl);
      cardEl.classList.add('lift');
      onMove = (dx, dy) => { c.x = Math.round(x0 + dx / k); c.y = Math.round(y0 + dy / k); cardEl.style.left = c.x + 'px'; cardEl.style.top = c.y + 'px'; };
      onUp = () => { cardEl.classList.remove('lift'); if (moved) redrawWorld(); else openFromBoard(c); };
    } else {  // empty space (or inside a group): pan
      const vx = BOARD.view.x, vy = BOARD.view.y;
      cv.classList.add('panning');
      onMove = (dx, dy) => { BOARD.view.x = vx + dx; BOARD.view.y = vy + dy; applyBoardView(); };
      onUp = () => cv.classList.remove('panning');
    }
    const mm = ev => {
      const dx = ev.clientX - start.x, dy = ev.clientY - start.y;
      if (!moved && Math.hypot(dx, dy) < 4) return;
      moved = true; onMove(dx, dy);
    };
    const mu = () => {
      removeEventListener('mousemove', mm); removeEventListener('mouseup', mu);
      onUp(); if (moved) saveBoard();
    };
    addEventListener('mousemove', mm); addEventListener('mouseup', mu);
  };
  // Dragging images in from the list on the left
  cv.ondragover = e => { if ([...e.dataTransfer.types].some(t => t === 'text/x-slice' || t === 'text/x-snap')) { e.preventDefault(); e.dataTransfer.dropEffect = 'copy'; } };
  cv.ondrop = e => {
    const id = e.dataTransfer.getData('text/x-slice'), snap = e.dataTransfer.getData('text/x-snap');
    if (!id && !snap) return;
    e.preventDefault();
    const p = toWorld(e);
    BOARD.cards.push({ id: uid('c'), ...(snap ? { snap } : { slice: id }), x: Math.round(p.x - CARD_W / 2), y: Math.round(p.y - 40) });
    saveBoard(); renderBoard();
  };
}

document.addEventListener('dragstart', e => {
  const it = e.target.closest?.('.tray-item[data-slice]');
  if (!it) return;
  e.dataTransfer.setData('text/x-slice', it.dataset.slice);
  e.dataTransfer.effectAllowed = 'copy';
});
document.addEventListener('input', e => {
  if (e.target.dataset.in === 'tray-q') { TRAY.q = e.target.value; $('#tray-list').innerHTML = trayList(); }
  const gid = e.target.dataset.gtitle;
  if (gid) { const g = BOARD.groups.find(g => g.id === gid); if (g) { g.title = e.target.value; e.target.size = Math.max(14, g.title.length + 1); saveBoard(); } }
});
document.addEventListener('keydown', e => {
  if (e.target.dataset?.gtitle && e.key === 'Enter') e.target.blur();
});

// ---------------------------------------------------------------- buttons

function groupListText(g) {
  const bySlide = new Map();
  groupCards(g).forEach(c => { const s = sliceById(c.slice); if (s) (bySlide.get(s.slide) || bySlide.set(s.slide, { s, ims: new Set() }).get(s.slide)).ims.add(s.im); });
  const lines = [...bySlide].sort((a, b) => a[0] - b[0]).map(([n, { s, ims }]) =>
    `${slideName(n)}${sliceMeta(s) ? ` (${sliceMeta(s)})` : ''}: ${[...ims].sort((a, b) => a - b).map(i => 'IM' + pad(i)).join(', ')}`);
  const snapLines = groupCards(g).filter(c => c.snap).map(c => SNAPS.list.find(x => x.id === c.snap)).filter(Boolean)
    .map(x => `Snap: ${slideName(x.slide)} IM${pad(x.im)}, ${chanLabel(x.channel)}${x.widthUm ? `, ${fmtUm(x.widthUm)} wide` : ''}${x.caption ? `: ${x.caption}` : ''} (${x.file})`);
  return `${g.title || 'Untitled group'}\n${[...lines, ...snapLines].join('\n') || '(no images yet)'}`;
}

Object.assign(ACT, {
  'b-tray': el => { TRAY.show = el.dataset.v; document.querySelector('.tray').innerHTML = trayHtml(); },
  'b-group': () => {
    const c = $('#bcanvas').getBoundingClientRect(), v = BOARD.view;
    newGroup((c.width / 2 - v.x) / v.k - 280, (c.height / 2 - v.y) / v.k - 160);
  },
  'b-zoom': el => { const c = $('#bcanvas'); zoomBoard(+el.dataset.k, c.clientWidth / 2, c.clientHeight / 2); },
  'b-fit': () => {
    const boxes = [...BOARD.groups.map(g => [g.x, g.y, g.x + g.w, g.y + g.h]), ...BOARD.cards.map(c => [c.x, c.y, c.x + CARD_W, c.y + CARD_H])];
    if (!boxes.length) return;
    const x0 = Math.min(...boxes.map(b => b[0])), y0 = Math.min(...boxes.map(b => b[1]));
    const x1 = Math.max(...boxes.map(b => b[2])), y1 = Math.max(...boxes.map(b => b[3]));
    const c = $('#bcanvas'), pad_ = 60;
    const k = Math.min(Math.max(Math.min((c.clientWidth - pad_ * 2) / (x1 - x0), (c.clientHeight - pad_ * 2) / (y1 - y0)), 0.15), 1.5);
    BOARD.view = { k, x: (c.clientWidth - (x1 - x0) * k) / 2 - x0 * k, y: (c.clientHeight - (y1 - y0) * k) / 2 - y0 * k };
    applyBoardView(); saveBoard();
  },
  'b-del': el => { BOARD.cards = BOARD.cards.filter(c => c.id !== el.dataset.id); saveBoard(); renderBoard(); },
  'b-gdel': el => {
    const g = BOARD.groups.find(g => g.id === el.dataset.id);
    if (!confirm(`Remove the group “${g.title || 'Untitled'}”?\n\nIts images stay on the board.`)) return;
    BOARD.groups = BOARD.groups.filter(x => x !== g); saveBoard(); renderBoard();
  },
  'b-color': el => {
    const g = BOARD.groups.find(g => g.id === el.dataset.id);
    g.color = GROUP_COLORS[(GROUP_COLORS.indexOf(g.color) + 1) % GROUP_COLORS.length];
    saveBoard(); redrawWorld();
  },
  'b-copy': el => {
    const g = BOARD.groups.find(g => g.id === el.dataset.id);
    navigator.clipboard.writeText(groupListText(g)).then(() => { el.textContent = 'Copied ✓'; setTimeout(() => { el.textContent = 'Copy list'; }, 1500); });
  },
  // From the viewer: put the open image on the board, in view, next to whatever was added last
  'b-add': () => {
    if (!S.v.open || !S.v.id) return;
    const v = BOARD.view, cv = $('#bcanvas');
    const w = cv?.clientWidth || innerWidth - 300, h = cv?.clientHeight || innerHeight - 64;
    const n = BOARD.cards.filter(c => c.fromViewer).length % 12;
    BOARD.cards.push({ id: uid('c'), slice: S.v.id, fromViewer: true,
      x: Math.round((w / 2 - v.x) / v.k - CARD_W / 2 + (n % 4) * 24), y: Math.round((h / 2 - v.y) / v.k - CARD_H / 2 + n * 18) });
    saveBoard(); renderViewer();
  },
});

loadBoard();

/* The Fluora flower: five petals, one per stain colour, that open as a project gets reviewed.
   A bud when nothing is marked yet, fully open when every image has a mark. It blooms open when
   the app starts, and its petals gently pulse while previews are being made. */
'use strict';

const PETALS = ['#00a1ff', '#00e05a', '#ffd700', '#ff3a1a', '#ff00ff'];

// How open petal i is (0..1) at a given review progress: all petals grow together, each a
// little behind the one before, so the flower stays balanced as it opens
function petalOpen(p, i) {
  const lag = 0.06, t = Math.min(Math.max((p - i * lag) / (1 - (PETALS.length - 1) * lag), 0), 1);
  return 1 - (1 - t) ** 2;
}

/* The flower as SVG. p = share of images reviewed (0..1).
   opts.size in px, opts.bloom = start closed and open up when shown, opts.busy = pulse. */
function flower(p, opts = {}) {
  const size = opts.size || 30;
  const petals = PETALS.map((c, i) => {
    const o = petalOpen(p || 0, i);
    const s = (0.3 + 0.7 * o).toFixed(3), a = 0.38 + 0.55 * o;
    return `<ellipse class="pt" rx="6" ry="13" cy="-10" fill="${c}" style="--a:${i * 72}deg;--s:${s};--i:${i};opacity:${a.toFixed(2)}"/>`;
  }).join('');
  return `<svg class="fl${opts.bloom ? ' closed' : ''}${opts.busy ? ' busy' : ''}" width="${size}" height="${size}" viewBox="0 0 46 46" aria-hidden="true">
    <g transform="translate(23 23)">${petals}<circle class="fl-c" r="${(2 + 2.2 * (p || 0)).toFixed(2)}" fill="#fff" opacity="${(0.35 + 0.55 * (p || 0)).toFixed(2)}"/></g></svg>`;
}

// Opens any flower drawn with bloom: true, a moment after it reaches the page
function bloomFlowers(root = document) {
  requestAnimationFrame(() => requestAnimationFrame(() => root.querySelectorAll('.fl.closed').forEach(f => f.classList.remove('closed'))));
}

const reviewProgress = slices => {
  const n = slices.length;
  return n ? slices.filter(s => (S.data.marks[s.id] || {}).status).length / n : 0;
};

// The header logo: this project's flower, with how far the review has got
let headerBloomed = false;
function renderLogo() {
  const el = document.querySelector('header .logo-fl');
  if (!el) return;
  const all = S.data.slices || [], p = reviewProgress(all);
  const b = S.data.build || {};
  const busy = b.phase && !['idle', 'done', 'error'].includes(b.phase);
  const done = all.filter(s => (S.data.marks[s.id] || {}).status).length;
  const key = `${p.toFixed(3)}|${busy}`;
  el.title = all.length ? `${Math.round(p * 100)}% reviewed · ${done} of ${all.length} images have a mark. The flower opens as you review.` : 'Fluora';
  if (el.dataset.key === key) return;
  el.dataset.key = key;
  el.innerHTML = flower(p, { size: 34, bloom: !headerBloomed, busy });
  if (!headerBloomed) { headerBloomed = true; bloomFlowers(el); }
}

// Images labelled without a slide number are slide 0
const slideName = n => (+n ? `Slide ${n}` : 'No slide number');

/* Updates. A moment after the app opens it asks GitHub whether a newer version has been
   published. If so, a small card asks whether to update; "Update now" downloads the new
   version, swaps it in and restarts the app. "Later" asks again the next time the app opens. */
const RELEASES_URL = 'https://github.com/charliechuckcharles/Fluora/releases/latest';
async function checkForUpdate() {
  let u;
  try { u = await api('/api/update/check', {}); } catch { return; }
  if (!u || !u.version) return;
  const app = document.title || 'Fluora';
  const card = document.createElement('div');
  card.id = 'update-card';
  card.setAttribute('role', 'dialog');
  card.innerHTML = `<b>${esc(app)} ${esc(u.version)} is available</b>
    <p>You have ${esc(u.current)}. Update now? It takes about a minute and the app restarts itself. Your marks, notes and snaps stay as they are.</p>
    ${u.notes ? `<p class="notes">${esc(u.notes)}</p>` : ''}
    <div class="foot"><button class="ghost" data-up="later">Later</button><button class="primary" data-up="now">Update now</button></div>`;
  document.body.appendChild(card);
  card.addEventListener('click', async e => {
    const b = e.target.closest('[data-up]');
    if (!b) return;
    if (b.dataset.up === 'later') { card.remove(); return; }
    card.innerHTML = `<b>Updating to ${esc(u.version)}…</b><p>Downloading the new version. ${esc(app)} will close and reopen by itself when it's ready.</p>`;
    try {
      await api('/api/update/install', {});
    } catch (err) {
      // the app restarting mid-request also lands here; only a real reply carries a reason
      if (!err || !err.message || /fetch|network|load failed/i.test(err.message)) return;
      card.innerHTML = `<b>The update didn't install</b><p>${esc(err.message)}</p>
        <p>You can still get it by hand: download the newest version from <span class="link">${esc(RELEASES_URL)}</span></p>
        <div class="foot"><button class="ghost" data-up="later">Close</button></div>`;
    }
  });
}
setTimeout(checkForUpdate, 1500);

/* What the setup screens need from the main app, for Fluora Renamer, which shows only those
   screens (Folder → Labels → rename). */
'use strict';
const IS_WIN = /Win/.test(navigator.userAgentData?.platform || navigator.platform);
const $ = sel => document.querySelector(sel);
const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
async function api(path, body) {
  const r = await fetch(path, body ? { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) } : {});
  const j = await r.json();
  if (!r.ok) throw new Error(j.error || r.statusText);
  return j;
}
const S = { data: { settings: {}, projects: { projects: [] } }, modalLocked: false };
const currentProject = () => ({ name: 'Lab file naming' });
function showModal(html) { $('#modal').innerHTML = `<div class="dialog">${html}</div>`; $('#modal').hidden = false; }
function closeModal() { $('#modal').hidden = true; }
let toastTimer;
function toast(msg, ms = 3500) {
  const t = $('#toast');
  t.innerHTML = esc(msg);
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { t.hidden = true; }, ms);
}
async function load() {}
function poll() {}
const ACT = { 'close-modal': closeModal };
document.addEventListener('click', e => {
  const el = e.target.closest('[data-act]');
  if (el) { e.preventDefault(); ACT[el.dataset.act]?.(el); }
  else if (e.target.id === 'modal') closeModal();
});
document.addEventListener('keydown', e => { if (e.key === 'Escape' && !$('#modal').hidden) closeModal(); });

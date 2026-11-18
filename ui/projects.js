/* Projects and "your account".
   A project is one set of slides: its own image folder, marks, notes, contrast, board and previews.
   Your name labels your marks, notes and snaps. Everything stays on this computer. */
'use strict';

const initials = n => (n || '?').trim().split(/\s+/).map(w => w[0]).slice(0, 2).join('').toUpperCase();
const projectList = () => S.data.projects?.projects || [];
const currentProject = () => projectList().find(p => p.id === S.data.project) || {};

// ---------------------------------------------------------------- header: project switcher + account

function renderProjectBtn() {
  const btn = $('#project-btn');
  if (btn) btn.innerHTML = `<span class="pname">${esc(currentProject().name || S.data.settings.projectName || 'My slides')}</span><span class="caret">▾</span>`;
  const me = $('#me');
  const name = S.data.profile?.name;
  if (me) me.innerHTML = name ? `<span class="avatar">${esc(initials(name))}</span><span class="me-name">${esc(name)}</span>`
    : '<span class="avatar">?</span><span class="me-name">Add your name</span>';
}

function toggleProjectMenu(force) {
  const menu = $('#project-menu');
  const open = force ?? menu.hidden;
  if (!open) { menu.hidden = true; return; }
  const list = projectList();
  menu.innerHTML = `<div class="pm-label">Projects</div>
    ${list.map(p => `<button class="pm-item ${p.id === S.data.project ? 'on' : ''}" data-act="project-switch" data-id="${esc(p.id)}">
      <span class="pm-fl" title="${p.total ? `${Math.round(p.reviewed / p.total * 100)}% reviewed` : 'No slides yet'}">${flower(p.total ? p.reviewed / p.total : 0, { size: 20 })}</span><span class="pm-name">${esc(p.name)}</span>
      <span class="pm-pct">${p.total ? Math.round(p.reviewed / p.total * 100) + '%' : ''}</span>${p.id === S.data.project ? '<span class="pm-check">✓</span>' : ''}</button>`).join('')}
    <div class="pm-sep"></div>
    <button class="pm-item" data-act="project-new"><span class="pm-plus">＋</span>New project…</button>
    <button class="pm-item" data-act="project-rename"><span class="pm-plus">✎</span>Rename this project</button>
    ${list.length > 1 ? '<button class="pm-item danger" data-act="project-remove"><span class="pm-plus">✕</span>Remove this project…</button>' : ''}`;
  menu.hidden = false;
}

document.addEventListener('click', e => {
  const menu = $('#project-menu');
  if (menu && !menu.hidden && !e.target.closest('#project-menu, #project-btn')) menu.hidden = true;
}, true);

// Switching reloads the page, so every tab starts fresh with the other project's slides
async function switchProject(id) {
  if (id === S.data.project) { toggleProjectMenu(false); return; }
  flushNote();
  if (typeof boardSaveTimer !== 'undefined' && boardSaveTimer) { clearTimeout(boardSaveTimer); await api('/api/board', BOARD).catch(() => {}); }
  await api('/api/projects', { action: 'switch', id });
  location.reload();
}

// ---------------------------------------------------------------- dialogs

function newProjectDialog() {
  toggleProjectMenu(false);
  showModal(`<h2>New project</h2>
    <p>A project is one set of slides, with its own marks, notes, board and brightness settings. Next you’ll choose its image folder and check the labels. You can switch projects at any time from the menu under the app’s name.</p>
    <div class="form">
      <label class="field"><span>Project name</span>
        <input type="text" data-k="name" placeholder="e.g. FISH slides, spring 2026" spellcheck="false"></label>
    </div>
    <div class="foot"><button class="ghost" data-act="close-modal">Cancel</button><button class="primary" data-act="project-create">Next: choose images</button></div>`);
  setTimeout(() => $('#modal [data-k="name"]')?.focus(), 30);
}

async function createProject() {
  const input = $('#modal [data-k="name"]'), name = input.value.trim();
  if (!name) { input.focus(); return; }
  await api('/api/projects', { action: 'create', name, imageFolder: '' });
  // the new project opens on its setup
  location.hash = 'setup';
  location.reload();
}

function renameProjectDialog() {
  toggleProjectMenu(false);
  const p = currentProject();
  showModal(`<h2>Rename project</h2>
    <div class="form"><label class="field"><span>Project name</span>
      <input type="text" data-k="name" value="${esc(p.name || '')}" spellcheck="false"></label></div>
    <div class="foot"><button class="ghost" data-act="close-modal">Cancel</button><button class="primary" data-act="project-rename-save">Save</button></div>`);
  setTimeout(() => { const i = $('#modal [data-k="name"]'); i?.focus(); i?.select(); }, 30);
}

async function removeProject() {
  toggleProjectMenu(false);
  const p = currentProject();
  if (!confirm(`Remove “${p.name}” from Fluora?\n\nIts marks, notes and board are moved to a “removed” folder in the app’s data, not deleted, so they can be brought back. Your image folder is not touched.`)) return;
  await api('/api/projects', { action: 'remove', id: p.id });
  location.reload();
}

// Your name: asked for the first time the app opens, changeable by clicking it in the header
function nameDialog(first) {
  S.modalLocked = !!first;
  const name = S.data.profile?.name || '';
  showModal(`${first ? `<div class="welcome-logo">${flower(1, { size: 56, bloom: true })}</div><h2>Welcome to Fluora</h2>` : '<h2>Your name</h2>'}
    <p>${first ? 'First, what should we call you? ' : ''}Your name labels the marks, notes and snaps you make, so it’s clear who reviewed what when you share them. It stays on this computer.</p>
    <div class="form"><label class="field"><span>Your name</span>
      <input type="text" data-k="name" value="${esc(name)}" placeholder="First and last name" autocomplete="name" spellcheck="false"></label></div>
    <div class="foot">${first ? '' : '<button class="ghost" data-act="close-modal">Cancel</button>'}<button class="primary" data-act="name-save">${first ? 'Continue' : 'Save'}</button></div>`);
  setTimeout(() => $('#modal [data-k="name"]')?.focus(), 30);
  bloomFlowers($('#modal'));
}

async function saveName() {
  const input = $('#modal [data-k="name"]'), name = input.value.trim();
  if (!name) { input.focus(); return; }
  const r = await api('/api/profile', { name });
  S.data.profile = r.profile;
  const wasFirst = S.modalLocked;
  S.modalLocked = false;
  closeModal();
  renderProjectBtn();
  // A brand-new user next needs to say where their slides are
  if (wasFirst && !S.data.slices.length && !S.data.settings.imageFolder) setupFirstProject();
}

function setupFirstProject() {
  showModal(`<h2>Your first project</h2>
    <p>A project is one set of slides. Give it a name, then choose where its images are. Fluora makes quick previews on this computer, so browsing is fast and works without the VPN.</p>
    <div class="form">
      <label class="field"><span>Project name</span>
        <input type="text" data-k="name" value="${esc(currentProject().name === 'My slides' ? '' : currentProject().name || '')}" placeholder="e.g. Paraffin sections, 2026" spellcheck="false"></label>
    </div>
    <div class="foot"><button class="ghost" data-act="close-modal">Later</button><button class="primary" data-act="first-project-save">Next: choose images</button></div>`);
  setTimeout(() => $('#modal [data-k="name"]')?.focus(), 30);
}

async function saveFirstProject() {
  const name = $('#modal [data-k="name"]').value.trim();
  if (name) {
    await api('/api/projects', { action: 'rename', id: S.data.project, name });
    await api('/api/settings', { projectName: name });
    await load();
  }
  closeModal();
  openSetup({ autoDetect: true });
}

Object.assign(ACT, {
  'project-menu': () => toggleProjectMenu(),
  'project-switch': el => switchProject(el.dataset.id).catch(e => alert(e.message)),
  'project-new': newProjectDialog,
  'project-create': () => createProject().catch(e => alert(`Couldn’t make the project: ${e.message}`)),
  'project-rename': renameProjectDialog,
  'project-rename-save': async () => {
    const name = $('#modal [data-k="name"]').value.trim();
    if (!name) return;
    await api('/api/projects', { action: 'rename', id: S.data.project, name }).catch(e => alert(e.message));
    closeModal(); await load();
  },
  'project-remove': () => removeProject().catch(e => alert(e.message)),
  me: () => nameDialog(false),
  'name-save': () => saveName().catch(e => alert(`Couldn’t save your name: ${e.message}`)),
  'first-project-save': () => saveFirstProject().catch(e => alert(e.message)),
});

document.addEventListener('keydown', e => {
  if (e.key !== 'Enter' || $('#modal').hidden || e.target.tagName !== 'INPUT') return;
  const primary = $('#modal .foot .primary');
  if (primary) { e.preventDefault(); primary.click(); }
});

function firstRun() {
  if (location.hash === '#setup') {
    history.replaceState(null, '', location.pathname);
    openSetup({ autoDetect: true });
    return;
  }
  if (!S.data.profile?.name) nameDialog(true);
}

/* Project setup, full window: Folder → Labels → Channels.
   Folder: find the images (and unpack CZIs with Fiji). Labels: one section per folder; its top
   row holds the values every image shares, each image row can change any of them. Optionally
   rename the files on the drive to the lab's naming (with undo). Channels: stain name and colour
   per channel. Then the previews are made. */
'use strict';

const SU = {
  open: false, step: 'folder', folder: '', labels: null, plan: null, channels: null, initials: '',
  columns: { sample: false, focus: false }, fiji: null, fijiJava: false, stamp: null, renamed: 0,
  job: null, checking: false, show: 'all', openGroups: new Set(), unpackSel: new Set(), dirty: false, note: '',
};

const SU_FIELDS = [
  { k: 'sample', label: 'Sample ID', opt: true, ph: 'e.g. 03p', w: 80,
    tip: 'Any extra ID that goes at the front of the name, like a paraffin block (03p). Optional.' },
  { k: 'cond', label: 'Condition', ph: 'e.g. E', w: 82,
    tip: 'The sample’s group, such as E (exposed or infected), C (control), Cad (cadaver) or D4 (day 4). Use your project’s own codes.' },
  { k: 'slide', label: 'Slide', ph: 'e.g. 47', w: 70, tip: 'The slide number. Each slide has one unique number (SL47).' },
  { k: 'im', label: 'Image', ph: '01', w: 62, perImage: true,
    tip: 'Image number on that slide: the 1st, 2nd, 3rd… area scanned (IM01, IM02). Filled in for you in scan order.' },
  { k: 'stains', label: 'Stains', ph: 'e.g. Phalloidin-DAPI', w: 160, tip: 'The dyes used, separated by dashes, like Phalloidin-Sytox-DAPI.' },
  { k: 'obj', label: 'Objective', ph: 'e.g. 20x', w: 76, tip: 'Magnification of the objective used, like 10x, 20x or 40x.' },
  { k: 'focus', label: 'Focus', opt: true, ph: 'e.g. HD', w: 72,
    tip: 'The region or focus of the image, if it’s a specific one: HD head, BR brain, WB whole body. Optional.' },
  { k: 'date', label: 'Date', ph: 'YYYYMMDD', w: 96, tip: 'The date it was imaged, as YYYYMMDD.' },
  { k: 'initials', label: 'Initials', ph: 'e.g. CNH', w: 76, tip: 'Who imaged it.' },
];
const SU_KINDS = [['image', 'Image'], ['label', 'Slide label'], ['overview', 'Overview'], ['skip', 'Leave out']];
const SU_STEPS_ALL = [['folder', 'Folder'], ['labels', 'Labels'], ['channels', 'Channels']];
// Fluora Renamer (the standalone app) stops after the labels and renaming
const suSteps = () => SU.mode === 'renamer' ? SU_STEPS_ALL.slice(0, 2) : SU_STEPS_ALL;

const ICON = {
  info: '<svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="6.3"/><path d="M8 7.2v4"/><circle cx="8" cy="4.9" r=".55" class="fill"/></svg>',
  check: '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8.4l2.9 2.9 6-6.3"/></svg>',
  caret: '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 4l4 4-4 4"/></svg>',
  warn: '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2.2l6.2 11H1.8z"/><path d="M8 6.4v3.3"/><circle cx="8" cy="11.6" r=".55" class="fill"/></svg>',
  folder: '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.8 4.2h4.4l1.4 1.6h6.6v6.9H1.8z"/></svg>',
  undo: '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4.5 6.5h6a3 3 0 010 6H7"/><path d="M6.8 4.2L4.5 6.5l2.3 2.3"/></svg>',
};

const fmt = n => Number(n || 0).toLocaleString();
const plural = (n, one, many = one + 's') => `${fmt(n)} ${n === 1 ? one : many}`;
const visibleFields = () => SU_FIELDS.filter(f => !f.opt || SU.columns[f.k]);
const groupFields = (g, e) => { const out = {}; SU_FIELDS.forEach(f => { out[f.k] = (e.fields[f.k] || '').trim() || (g.shared[f.k] || ''); }); return out; };
const relOf = p => (SU.labels?.root && p.startsWith(SU.labels.root) ? p.slice(SU.labels.root.length).replace(/^[\\/]/, '') : p);

// ---------------------------------------------------------------- open / close

async function openSetup(opts = {}) {
  const r = await api('/api/setup/load', {});
  SU.mode = opts.renamer ? 'renamer' : 'fluora';
  Object.assign(SU, { open: true, folder: r.folder || '', labels: r.labels, initials: r.initials || '', fiji: r.fiji,
    fijiJava: r.fijiJava, plan: null, channels: null, stamp: null, renamed: 0, job: null, dirty: false, note: '', show: 'all' });
  SU.columns = { sample: false, focus: false, ...(r.columns || {}) };
  SU.openGroups = new Set();
  $('#setup').hidden = false;
  document.body.classList.add('su-on');
  if (SU.labels) {
    revealUsedColumns();
    goStep('labels');
    // look for images added since the labels were saved
    if (SU.folder) startDetect(true, opts.refresh ? 'Looking for new images…' : 'Checking the folder for new images…');
  } else {
    goStep('folder');
    if (SU.folder && opts.autoDetect) startDetect(false);
  }
}

function closeSetup(force) {
  if (!force && SU.dirty && !confirm('Close without saving these labels?')) return;
  if (SU.job?.phase === 'running' && SU.job.kind === 'rename') { alert('Wait for the renaming to finish first.'); return; }
  SU.open = false;
  $('#setup').hidden = true;
  document.body.classList.remove('su-on');
  hideTip();
}

function revealUsedColumns() {
  for (const g of SU.labels?.groups || []) {
    if (g.shared.sample || g.entries.some(e => e.fields.sample)) SU.columns.sample = true;
    if (g.shared.focus || g.entries.some(e => e.fields.focus)) SU.columns.focus = true;
  }
}

function goStep(step) {
  SU.step = step;
  if (step === 'labels') schedulePlan(0);
  if (step === 'channels') loadChannels();
  renderSetup();
  $('#setup .su-body')?.scrollTo(0, 0);
}

// ---------------------------------------------------------------- background jobs

let jobTimer = null;
async function pollJob() {
  clearTimeout(jobTimer);
  let j;
  try { j = await api('/api/setup/status'); } catch { jobTimer = setTimeout(pollJob, 1500); return; }
  SU.job = j;
  if (j.phase === 'running') {
    renderJob();
    jobTimer = setTimeout(pollJob, 500);
    return;
  }
  const kind = j.kind;
  if (j.phase === 'error') {
    SU.checking = false;
    SU.note = j.error;
    if (kind === 'detect' && SU.step === 'labels') SU.note = `Couldn’t check the folder for new images: ${j.error}`;
    renderSetup();
    return;
  }
  if (j.phase !== 'done') return;
  if (kind === 'detect') onDetected(j.result);
  if (kind === 'unpack') {
    toastSetup(j.result.cancelled ? 'Unpacking stopped' : `Unpacked ${plural(j.result.unpacked.length, 'CZI file')}`);
    SU.unpackSel.clear();
    if (SU.unpackTo) {
      // the TIFFs are in the new folder: the project uses that from now on
      SU.folder = SU.unpackTo;
      SU.labels = null;
      SU.unpackTo = null;
      startDetect(false);
    } else startDetect(false);
  }
  if (kind === 'rename') onRenamed(j.result);
  if (kind === 'undo') onUndone(j.result);
}

async function startDetect(merge, msg) {
  const folder = SU.folder.trim().replace(/^['"]|['"]$/g, '');
  if (!folder) { $('#setup [data-su="folder"]')?.focus(); return; }
  SU.folder = folder;
  SU.note = '';
  SU.checking = merge;
  SU.checkingMsg = msg || '';
  if (SU.mode === 'renamer') api('/api/setup/prefs', { lastFolder: folder, columns: SU.columns }).catch(() => {});
  try { await api('/api/setup/detect', { folder, initials: SU.initials }); }
  catch (e) { SU.note = e.message; renderSetup(); return; }
  SU.job = { kind: 'detect', phase: 'running', done: 0, message: 'Starting…' };
  renderSetup();
  pollJob();
}

async function onDetected(det) {
  const merging = SU.checking;
  SU.checking = false;
  if (merging && SU.labels) {
    // merge into the labels as they are on screen, so anything typed meanwhile is kept
    try { det = await api('/api/setup/merge', { labels: SU.labels, fresh: det }); }
    catch (e) { SU.note = e.message; renderSetup(); return; }
    const fresh = det.groups.reduce((n, g) => n + g.entries.filter(e => e.new).length, 0);
    SU.labels = det;
    if (fresh) { SU.dirty = true; SU.note = ''; toastSetup(`${plural(fresh, 'new image')} found, highlighted below`); }
    else SU.note = '';
    det.groups.forEach((g, i) => { if (g.new || g.entries.some(e => e.new)) SU.openGroups.add(i); });
  } else {
    SU.labels = det;
    SU.dirty = true;
    revealUsedColumns();
    // open the folders that need something; fully named ones start folded
    det.groups.forEach((g, i) => { if (g.entries.some(e => !e.named)) SU.openGroups.add(i); });
    if (det.groups.length <= 3) det.groups.forEach((_, i) => SU.openGroups.add(i));
  }
  if (SU.step === 'labels') schedulePlan(0);
  renderSetup();
}

// ---------------------------------------------------------------- the plan (new names + problems)

let planTimer = null, planSeq = 0;
function schedulePlan(delay = 300) {
  clearTimeout(planTimer);
  planTimer = setTimeout(async () => {
    if (!SU.labels) return;
    const seq = ++planSeq;
    try {
      const p = await api('/api/setup/plan', { labels: SU.labels });
      if (seq !== planSeq) return;
      SU.plan = p;
      if (SU.step === 'labels') paintPlan();
    } catch (e) { SU.note = e.message; }
  }, delay);
}

// Problems Fluora itself cares about (renaming has stricter needs, from the plan)
function viewProblems() {
  const out = new Map(), seen = new Map();
  (SU.labels?.groups || []).forEach((g, gi) => {
    if (g.skip) return;
    g.entries.forEach((e, ei) => {
      if (e.kind !== 'image') return;
      const f = groupFields(g, e);
      // only an image number is needed to show it; the rest is for renaming
      if (!/\d/.test(f.im)) { out.set(`${gi}:${ei}`, 'Needs an image number'); return; }
      const key = `${g.folder}|${+f.slide.replace(/\D/g, '')}|${+f.im.replace(/\D/g, '')}`;
      if (seen.has(key)) out.set(`${gi}:${ei}`, 'Same slide and image number as another image in this folder');
      else seen.set(key, true);
    });
  });
  return out;
}

function problemsFor(gi, ei) {
  const list = (SU.plan?.problems || []).filter(p => p.group === gi && p.entry === ei);
  const view = viewProblems().get(`${gi}:${ei}`);
  return { view, rename: list.filter(p => p.kind !== 'note') };
}

// ---------------------------------------------------------------- rendering

function renderSetup() {
  const root = $('#setup');
  if (!root || !SU.open) return;
  const steps = suSteps(), idx = steps.findIndex(s => s[0] === SU.step);
  const renamer = SU.mode === 'renamer';
  const pname = currentProject().name || S.data.settings.projectName || 'this project';
  root.innerHTML = `
    <div class="su-top">
      <div class="su-brand">${flower(1, { size: 26 })}<div><b>${renamer ? 'Fluora Renamer' : 'Set up'}</b><span>${esc(pname)}</span></div></div>
      <ol class="su-rail">${steps.map(([k, label], i) => `<li class="${i < idx ? 'done' : i === idx ? 'on' : ''}">
        <button data-act="su-step" data-s="${k}" ${canGo(k) ? '' : 'disabled'}><span class="su-dot">${i < idx ? ICON.check : i + 1}</span>${label}</button></li>`).join('<li class="su-line" aria-hidden="true"></li>')}</ol>
      ${renamer ? '<span class="su-close"></span>' : '<button class="ghost su-close" data-act="su-close">Close</button>'}
    </div>
    <div class="su-body">${SU.step === 'folder' ? folderStep() : SU.step === 'labels' ? labelsStep() : channelsStep()}</div>
    <div class="su-foot">${footer()}</div>`;
  if (SU.step === 'labels') paintPlan();
}

function canGo(step) {
  if (step === 'folder') return true;
  if (step === 'channels' && SU.mode === 'renamer') return false;
  if (step === 'labels') return !!SU.labels;
  return !!SU.labels && viewProblems().size === 0;
}

function jobLine() {
  const j = SU.job;
  if (!j || j.phase !== 'running') return '';
  const pct = j.total ? Math.round(j.done / j.total * 100) : null;
  return `<div class="su-job" role="status"><div class="su-bar ${pct == null ? 'indet' : ''}"><i style="width:${pct ?? 30}%"></i></div>
    <span>${esc(j.message || 'Working…')}</span>${j.kind === 'unpack' ? '<button class="ghost" data-act="su-cancel">Stop</button>' : ''}</div>`;
}
function renderJob() {
  const el = $('#setup .su-jobslot');
  if (el) el.innerHTML = jobLine();
  const f = $('#setup .su-foot');
  if (f && SU.step !== 'folder') f.innerHTML = footer();
}

// ---------- step 1: folder

function folderStep() {
  const det = SU.labels, running = SU.job?.phase === 'running';
  const images = det ? det.groups.reduce((n, g) => n + g.entries.filter(e => e.kind === 'image').length, 0) : 0;
  const needs = det ? det.groups.filter(g => g.entries.some(e => !e.named)).length : 0;
  return `<section class="su-narrow">
    <h1>Where are this project’s images?</h1>
    <p class="su-lede">Choose the folder on the cnelya drive, or anywhere else. Fluora looks in it and every folder inside it. Files are only read, unless you later choose to rename them.</p>
    <div class="su-pick">
      <input type="text" data-su="folder" value="${esc(SU.folder)}" placeholder="${IS_WIN ? 'Z:\\…\\My slides' : '/Volumes/cnelya_lab/…'}" spellcheck="false" aria-label="Image folder">
      <button data-act="su-pick">${ICON.folder}Choose…</button>
      <button class="primary" data-act="su-detect" ${running ? 'disabled' : ''}>${det ? 'Look again' : 'Look for images'}</button>
    </div>
    <p class="su-help">On the lab drive? Connect the VPN and the drive first.</p>
    <div class="su-jobslot">${jobLine()}</div>
    ${SU.note ? `<div class="su-note bad">${ICON.warn}<span>${esc(SU.note)}</span></div>` : ''}
    ${det && !(running && SU.job.kind === 'detect') ? `
      <div class="su-found">
        <h2>${images ? `Found ${plural(images, 'image')} in ${plural(det.groups.length, 'folder')}` : 'No images found yet'}</h2>
        <p>${needs ? `${plural(needs, 'folder')} ${needs === 1 ? 'has' : 'have'} files without the lab’s naming. You’ll label ${needs === 1 ? 'it' : 'them'} next.` : images ? 'Every file already follows the lab’s naming.' : ''}</p>
        ${det.groups.length ? `<div class="su-flist">${det.groups.map(g => {
          const n = g.entries.filter(e => e.kind === 'image').length, unnamed = g.entries.some(e => !e.named), split = g.entries.some(e => e.split);
          return `<div class="su-frow"><span class="su-fname">${ICON.folder}${esc(g.rel || 'Chosen folder')}</span>
            <span class="su-fcount">${plural(n, 'image')}</span>
            <span class="su-tag ${unnamed ? 'warn' : 'good'}">${unnamed ? 'Needs labels' : split ? 'One file per stain' : 'Lab naming'}</span></div>`;
        }).join('')}</div>` : ''}
      </div>
      ${det.packed.length ? unpackCard(det) : ''}` : ''}
  </section>`;
}

function unpackCard(det) {
  const can = SU.fiji && SU.fijiJava;
  const ro = (det.packedReadOnly || []).length > 0;
  const needDest = ro && !SU.unpackDest;
  return `<div class="su-unpack">
    <h2>${plural(det.packed.length, 'CZI file')} not unpacked yet</h2>
    <p>Fiji can save each scan inside a CZI as TIFF files next to it (<span class="mono">name_series0.tif</span>, <span class="mono">name_series1.tif</span>…), the same as the lab’s Unpack macro. Fluora can then show them. Large scans take a few minutes each.</p>
    <div class="su-czis">${det.packed.map(p => `<label class="su-czi"><input type="checkbox" data-su-czi="${esc(p)}" ${SU.unpackSel.has(p) ? 'checked' : ''}>
      <span class="mono">${esc(relOf(p))}</span></label>`).join('')}</div>
    ${ro ? `<div class="su-note">${ICON.warn}<span>You can’t save files in ${det.packedReadOnly.length === 1 ? 'this folder' : 'some of these folders'}: it’s read-only for you, like Raw_data on the lab drive. Choose a folder to save the unpacked TIFFs in instead, such as one in your Analysis folder. The CZIs stay where they are, and this project will use the new folder.</span></div>
      <div class="su-pick su-dest"><input type="text" data-su="dest" value="${esc(SU.unpackDest || '')}" placeholder="Folder to save the TIFFs in" spellcheck="false" aria-label="Folder to save the unpacked TIFFs in">
        <button data-act="su-pick-dest">${ICON.folder}Choose…</button></div>` : ''}
    ${can ? `<div class="su-row-actions"><button data-act="su-czi-all">Select all</button>
        <button class="primary" data-act="su-unpack" ${SU.unpackSel.size && SU.job?.phase !== 'running' && !needDest ? '' : 'disabled'} ${needDest ? 'title="Choose a folder to save the TIFFs in first"' : ''}>Unpack ${SU.unpackSel.size ? plural(SU.unpackSel.size, 'file') : 'selected'} with Fiji</button></div>`
      : `<div class="su-note">${ICON.warn}<span>Fiji wasn’t found on this computer. Install Fiji, or set where it is in Settings, to unpack CZI files here. You can also skip this.</span></div>`}
  </div>`;
}

// ---------- step 2: labels

function tipBtn(text, label) {
  return `<button type="button" class="su-tipbtn" data-tip="${esc(text)}" aria-label="About ${esc(label)}">${ICON.info}</button>`;
}

function gridCols() {
  return ['104px', ...visibleFields().map(f => `${f.w}px`), 'minmax(280px, 1fr)'].join(' ');
}

function labelsStep() {
  const det = SU.labels;
  const fields = visibleFields();
  const vp = viewProblems();
  const groups = det.groups.map((g, gi) => ({ g, gi })).filter(({ g, gi }) => SU.show === 'all'
    || g.entries.some((e, ei) => vp.has(`${gi}:${ei}`) || e.new || !e.named) || g.new)
    // folders with new images first
    .sort((a, b) => (b.g.new || b.g.entries.some(e => e.new)) - (a.g.new || a.g.entries.some(e => e.new)));
  return `<section class="su-wide">
    <div class="su-head">
      <div><h1>Label images</h1>
        <p class="su-lede">The top row of each folder holds what all its images share; change any image on its own row. Names that already follow the lab’s format were read for you, and guesses are marked with a dotted line.</p></div>
      <div class="su-tools">
        <div class="su-toggle" role="group" aria-label="Optional columns"><span>Columns</span>
          ${SU_FIELDS.filter(f => f.opt).map(f => `<button class="chip ${SU.columns[f.k] ? 'on' : ''}" data-act="su-col" data-k="${f.k}" aria-pressed="${!!SU.columns[f.k]}">${f.label}</button>`).join('')}</div>
        <div class="seg su-show"><button class="${SU.show === 'all' ? 'on' : ''}" data-act="su-show" data-v="all">All folders</button><button class="${SU.show === 'attn' ? 'on' : ''}" data-act="su-show" data-v="attn">Needs a look</button></div>
      </div>
    </div>
    <div class="su-jobslot">${SU.checking ? jobLine() : ''}</div>
    ${SU.note ? `<div class="su-note">${ICON.warn}<span>${esc(SU.note)}</span></div>` : ''}
    <div class="su-table" style="--cols:${gridCols()}">
      <div class="su-thead" role="row"><div>Type</div>${fields.map(f => `<div>${f.label}${f.opt ? '' : ''}${tipBtn(f.tip, f.label)}</div>`).join('')}<div>New name</div></div>
      ${groups.length ? groups.map(({ g, gi }) => suGroupHtml(g, gi, fields)).join('') : '<div class="su-empty">Nothing needs a look. Switch to All folders to see everything.</div>'}
    </div>
  </section>`;
}

function suGroupHtml(g, gi, fields) {
  const open = SU.openGroups.has(gi);
  const imgs = g.entries.filter(e => e.kind === 'image').length;
  const fresh = g.entries.filter(e => e.new).length;
  const hint = [g.filters?.length ? `Channels in the files: ${g.filters.join(' · ')}` : '',
    g.um ? `${g.um.toFixed(3)} µm per pixel` : '', g.czis?.length ? plural(g.czis.length, 'CZI') : ''].filter(Boolean).join('  ·  ');
  return `<section class="su-group ${open ? 'open' : ''} ${g.skip ? 'skip' : ''}" data-g="${gi}">
    <div class="su-ghead">
      <button class="su-fold" data-act="su-fold" data-g="${gi}" aria-expanded="${open}">${ICON.caret}<span class="su-gname">${esc(g.rel || 'Chosen folder')}</span></button>
      <span class="su-gmeta">${plural(imgs, 'image')}${fresh ? ` · <b class="su-new">${fmt(fresh)} new</b>` : ''}</span>
      ${g.readOnly ? '<span class="su-tag" title="Your account can’t change files in this folder, so its files keep their names. Fluora can still show them.">Read-only</span>' : ''}
      <span class="su-gprob" id="su-gp-${gi}"></span>
      <label class="su-skip"><input type="checkbox" data-su-skip="${gi}" ${g.skip ? 'checked' : ''}> Leave out this folder</label>
    </div>
    ${g.skip ? '' : `<div class="su-row su-shared" role="row">
      <div class="su-kind">Every image</div>
      ${fields.map(f => f.perImage ? '<div class="su-auto">per image</div>'
        : `<div><input data-g="${gi}" data-e="-1" data-f="${f.k}" value="${esc(g.shared[f.k] || '')}" placeholder="${esc(f.ph)}"
            class="${(g.guessed || []).includes(f.k) ? 'guess' : ''}" ${(g.guessed || []).includes(f.k) ? 'title="Guessed by Fluora. Check it."' : ''} aria-label="${f.label} for every image in ${esc(g.rel || 'this folder')}" spellcheck="false"></div>`).join('')}
      <div class="su-hint">${esc(hint)}</div>
    </div>
    ${open ? g.entries.map((e, ei) => entryHtml(g, gi, e, ei, fields)).join('') : ''}`}
  </section>`;
}

function entryHtml(g, gi, e, ei, fields) {
  const first = e.parts[0]?.file.name || '';
  const sizes = e.split ? e.parts.map(p => p.tag).join(' · ') : e.parts.length > 1 ? `${e.parts.length} sizes` : 'one size';
  return `<div class="su-row ${e.kind === 'skip' ? 'off' : ''} ${e.new ? 'is-new' : ''}" role="row" data-e="${ei}">
    <div class="su-kind"><select data-g="${gi}" data-e="${ei}" data-f="kind" aria-label="What ${esc(first)} is">${SU_KINDS.map(([k, l]) => `<option value="${k}" ${e.kind === k ? 'selected' : ''}>${l}</option>`).join('')}</select></div>
    ${fields.map(f => {
      const special = e.kind === 'label' || e.kind === 'overview';
      if (e.kind === 'skip' || (special && ['im', 'stains', 'obj', 'focus'].includes(f.k))) return '<div></div>';
      return `<div><input data-g="${gi}" data-e="${ei}" data-f="${f.k}" value="${esc(e.fields[f.k] || '')}" placeholder="${esc(f.perImage ? f.ph : g.shared[f.k] || '')}" aria-label="${f.label}" spellcheck="false"></div>`;
    }).join('')}
    <div class="su-name">${e.new ? '<span class="su-newpill">New</span>' : ''}<span class="su-new-name mono" id="su-n-${gi}-${ei}"></span>
      <span class="su-old mono" title="${esc(e.parts.map(p => p.file.name).join('\n'))}">${esc(first)}<em>${esc(sizes)}</em></span>
      <span class="su-prob" id="su-p-${gi}-${ei}"></span></div>
  </div>`;
}

// Fills in the new names and problems from the latest plan without redrawing the inputs
function paintPlan() {
  const p = SU.plan, det = SU.labels;
  if (!det) return;
  const vp = viewProblems();
  det.groups.forEach((g, gi) => {
    let n = 0;
    g.entries.forEach((e, ei) => {
      const nameEl = document.getElementById(`su-n-${gi}-${ei}`), probEl = document.getElementById(`su-p-${gi}-${ei}`);
      const { view, rename } = problemsFor(gi, ei);
      if (view || rename.length) n++;
      if (nameEl) {
        const nn = p?.names?.[gi]?.[ei]?.[0], old = e.parts[0]?.file.name;
        const missing = rename.some(x => x.kind === 'missing');
        nameEl.textContent = e.kind === 'skip' ? 'Left out' : missing ? 'New name shows once the blanks are filled in'
          : nn && nn !== old ? nn : nn === old ? 'Name already right' : '';
        nameEl.classList.toggle('same', !nn || nn === old || e.kind === 'skip' || missing);
      }
      if (probEl) {
        probEl.innerHTML = view ? `${ICON.warn}${esc(view)}` : rename.length ? `<span class="soft">${esc(rename[0].message)} to rename</span>` : '';
        probEl.classList.toggle('bad', !!view);
      }
    });
    const gp = document.getElementById(`su-gp-${gi}`);
    if (gp) gp.innerHTML = n && !g.skip ? `${ICON.warn}${plural(n, 'image')} to check` : '';
  });
  const f = $('#setup .su-foot');
  if (f) f.innerHTML = footer();
  void vp;
}

// ---------- step 3: channels

async function loadChannels() {
  if (SU.channels) return;
  try {
    SU.channels = await api('/api/setup/channels', { labels: SU.labels });
    SU.channels.forEach(c => { c.orig = { stain: c.stain, color: c.color }; });
  } catch (e) { SU.note = e.message; SU.channels = []; }
  if (SU.step === 'channels') renderSetup();
}

function channelsStep() {
  if (!SU.channels) return `<section class="su-narrow"><h1>Channels</h1><div class="su-skel"></div><div class="su-skel"></div><div class="su-skel"></div></section>`;
  return `<section class="su-narrow">
    <h1>Channels</h1>
    <p class="su-lede">How each channel is named and coloured in Fluora. The names come from the filters in the files and the stains in your labels. Change anything that’s wrong; nothing here changes your files.</p>
    ${SU.channels.length ? `<div class="su-chans">
      <div class="su-chead"><div>Colour</div><div>Stain name</div><div>From filter</div><div>Images</div><div></div></div>
      ${SU.channels.map((c, i) => `<div class="su-chan">
        <label class="su-swatch" style="--c:${esc(c.color)}"><input type="color" value="${esc(c.color)}" data-ch="${i}" data-k="color" aria-label="Colour for ${esc(c.stain)}"></label>
        <input type="text" value="${esc(c.stain)}" data-ch="${i}" data-k="stain" aria-label="Stain name for filter ${esc(c.filters.join(', '))}" spellcheck="false">
        <span class="mono su-filt">${esc(c.filters.join(', '))}</span>
        <span class="su-count">${fmt(c.images)}</span>
        <button class="ghost" data-act="su-ch-reset" data-ch="${i}" ${c.stain === c.orig.stain && c.color === c.orig.color ? 'hidden' : ''}>${ICON.undo}Reset</button>
      </div>`).join('')}
    </div>
    <div class="su-preview" aria-hidden="true">${SU.channels.map(c => `<span style="--c:${esc(c.color)}">${esc(c.stain)}</span>`).join('')}</div>`
    : '<p class="su-help">No channels yet. Go back and include at least one image.</p>'}
  </section>`;
}

// ---------- footer

function footer() {
  const running = SU.job?.phase === 'running';
  if (SU.step === 'folder') {
    return `<span class="su-status"></span>
      <button class="primary" data-act="su-next" ${SU.labels && !running ? '' : 'disabled'}>Next: Labels</button>`;
  }
  if (SU.step === 'labels') {
    const p = SU.plan, vp = viewProblems();
    const renames = p?.renames?.length || 0, blocking = p ? p.problems.filter(x => x.kind !== 'note').length : 0;
    const renaming = running && ['rename', 'undo'].includes(SU.job.kind);
    // images whose blanks stop them being renamed (they aren't counted in renames)
    const needLabels = p ? new Set(p.problems.filter(x => x.kind !== 'note' && x.entry != null).map(x => `${x.group}:${x.entry}`)).size : 0;
    const left = renaming ? `<span class="su-status">${esc(SU.job.message || 'Renaming…')} ${SU.job.total ? `${fmt(SU.job.done)} of ${fmt(SU.job.total)}` : ''}</span>`
      : SU.stamp ? `<span class="su-status good">${ICON.check}Renamed ${plural(SU.renamed, 'file')}. <button class="ghost" data-act="su-undo">${ICON.undo}Undo rename</button></span>`
      : `<span class="su-status">${vp.size ? `<b class="bad">${plural(vp.size, 'image')} need${vp.size === 1 ? 's' : ''} an image number</b>` : p ? (renames ? `${plural(renames, 'file')} would get a new name` : needLabels ? `${plural(needLabels, 'image')} need${needLabels === 1 ? 's' : ''} labels before renaming` : 'All names already follow the lab’s format') : 'Checking names…'}${blocking && renames ? ` · ${fmt(blocking)} to fill in before renaming` : ''}</span>`;
    const renameBtn = (cls) => `<button class="${cls}" data-act="su-rename" ${renames && !blocking && !running ? '' : 'disabled'} title="${blocking ? 'Fill in the missing values first' : renames ? 'Give the files on the drive the lab’s names' : 'Nothing to rename'}">Rename files${SU.mode === 'renamer' ? (renames ? ` (${fmt(renames)})` : '') : ' on the drive…'}</button>`;
    if (SU.mode === 'renamer') {
      return `${left}<button class="ghost" data-act="su-step" data-s="folder">Back</button>
        ${SU.stamp && !renames ? '<button class="primary" data-act="su-another">Rename another folder</button>' : renameBtn('primary')}`;
    }
    return `${left}<button class="ghost" data-act="su-step" data-s="folder">Back</button>
      ${renameBtn('')}
      <button class="primary" data-act="su-next" ${vp.size || renaming ? 'disabled' : ''}>Next: Channels</button>`;
  }
  return `<span class="su-status">${SU.note ? `<b class="bad">${esc(SU.note)}</b>` : ''}</span><button class="ghost" data-act="su-step" data-s="labels">Back</button>
    <button class="primary" data-act="su-save" ${SU.channels?.length ? '' : 'disabled'}>Save and make previews</button>`;
}

// ---------------------------------------------------------------- renaming

function renameDialog() {
  const p = SU.plan;
  const folders = new Set(p.renames.map(r => r.folder)).size;
  const sample = p.renames.slice(0, 6);
  showModal(`<h2>Rename ${plural(p.renames.length, 'file')} on the drive?</h2>
    <p>In ${plural(folders, 'folder')}. Each folder keeps a list of the old names (<span class="mono">.fluora-renames.json</span>), so you can undo this here or later.</p>
    <div class="su-rn">${sample.map(r => `<div><span class="mono old">${esc(r.from)}</span><span class="mono">${esc(r.to)}</span></div>`).join('')}
      ${p.renames.length > sample.length ? `<div class="more">and ${plural(p.renames.length - sample.length, 'more file')}</div>` : ''}</div>
    <p class="su-help">Anything else that points to these files by name (a Zen project, a script) will need the new names. Keep the VPN connected until it finishes.</p>
    <div class="foot"><button class="ghost" data-act="close-modal">Cancel</button><button class="primary" data-act="su-rename-go">Rename files</button></div>`);
}

async function startRename() {
  closeModal();
  try { await api('/api/setup/rename', { labels: SU.labels }); }
  catch (e) { alert(e.message); return; }
  SU.job = { kind: 'rename', phase: 'running', message: 'Checking the new names against the drive…' };
  renderSetup();
  pollJob();
}

function onRenamed(r) {
  if (SU.mode === 'renamer') {
    const i = SU.labels.groups.map(g => g.shared.initials).find(Boolean);
    if (i) api('/api/setup/prefs', { initials: i }).catch(() => {});
  }
  SU.labels = r.labels;
  SU.stamp = r.stamp;
  SU.renamed = r.renamed;
  SU.dirty = true;
  if (r.failed.length) alert(`Renamed ${plural(r.renamed, 'file')}, but ${plural(r.failed.length, 'file')} couldn’t be renamed:\n\n${r.failed.slice(0, 8).join('\n')}${r.failed.length > 8 ? '\n…' : ''}`);
  schedulePlan(0);
  renderSetup();
}

async function startUndo() {
  if (!confirm(`Put back the old names of the ${plural(SU.renamed, 'file')} just renamed?`)) return;
  try { await api('/api/setup/undo', { labels: SU.labels, stamp: SU.stamp }); }
  catch (e) { alert(e.message); return; }
  SU.job = { kind: 'undo', phase: 'running', message: 'Putting the old names back…' };
  renderSetup();
  pollJob();
}

function onUndone(r) {
  SU.labels = r.labels;
  SU.stamp = null;
  if (r.failed.length) alert(`Put back ${plural(r.restored, 'name')}, but ${plural(r.failed.length, 'file')} couldn’t be changed:\n\n${r.failed.slice(0, 8).join('\n')}`);
  else toastSetup(`Put back ${plural(r.restored, 'old name')}`);
  schedulePlan(0);
  renderSetup();
}

// ---------------------------------------------------------------- saving

async function saveSetup() {
  const channels = SU.channels.map(c => ({ guess: c.guess, stain: c.stain, color: c.color }));
  const initials = (() => {
    const counts = {};
    SU.labels.groups.forEach(g => { if (g.shared.initials) counts[g.shared.initials] = (counts[g.shared.initials] || 0) + 1; });
    return Object.entries(counts).sort((a, b) => b[1] - a[1])[0]?.[0] || SU.initials;
  })();
  const btn = $('#setup [data-act="su-save"]');
  if (btn) { btn.disabled = true; btn.textContent = 'Saving…'; }
  try {
    await api('/api/setup/save', { labels: SU.labels, channels, folder: SU.folder, initials, columns: SU.columns });
  } catch (e) { alert(`Couldn’t save: ${e.message}`); if (btn) { btn.disabled = false; btn.textContent = 'Save and make previews'; } return; }
  closeSetup(true);
  await load();
  poll();
}

// ---------------------------------------------------------------- floating tips and toasts

function showTip(btn) {
  const t = $('#su-tip');
  t.textContent = btn.dataset.tip;
  t.hidden = false;
  const r = btn.getBoundingClientRect(), w = Math.min(280, innerWidth - 24);
  t.style.maxWidth = `${w}px`;
  const tw = t.offsetWidth;
  t.style.left = `${Math.max(12, Math.min(r.left + r.width / 2 - tw / 2, innerWidth - tw - 12))}px`;
  t.style.top = `${r.bottom + 8}px`;
}
function hideTip() { const t = $('#su-tip'); if (t) t.hidden = true; }

function toastSetup(msg) {
  if (typeof toast === 'function') toast(msg);
}

// ---------------------------------------------------------------- events

Object.assign(ACT, {
  'su-close': () => closeSetup(),
  'su-step': el => { if (canGo(el.dataset.s)) goStep(el.dataset.s); },
  'su-next': () => goStep(SU.step === 'folder' ? 'labels' : 'channels'),
  'su-pick': async () => {
    const r = await api('/api/pick-folder', { start: SU.folder });
    if (r.path) { SU.folder = r.path; renderSetup(); startDetect(false); }
  },
  'su-detect': () => startDetect(false),
  'su-pick-dest': async () => {
    const r = await api('/api/pick-folder', { start: SU.unpackDest || '', title: 'Choose where to save the unpacked TIFFs' });
    if (r.path) { SU.unpackDest = r.path; renderSetup(); }
  },
  'su-cancel': () => api('/api/setup/cancel', {}),
  'su-czi-all': () => { SU.labels.packed.forEach(p => SU.unpackSel.add(p)); renderSetup(); },
  'su-unpack': async () => {
    const dest = (det => (det.packedReadOnly || []).length ? SU.unpackDest : null)(SU.labels);
    SU.unpackTo = dest || null;
    try { await api('/api/setup/unpack', { czis: [...SU.unpackSel], dest, root: SU.labels.root }); }
    catch (e) { alert(e.message); return; }
    SU.job = { kind: 'unpack', phase: 'running', message: 'Starting Fiji…' };
    renderSetup();
    pollJob();
  },
  'su-col': el => { SU.columns[el.dataset.k] = !SU.columns[el.dataset.k]; renderSetup(); },
  'su-show': el => { SU.show = el.dataset.v; renderSetup(); },
  'su-fold': el => { const g = +el.dataset.g; SU.openGroups.has(g) ? SU.openGroups.delete(g) : SU.openGroups.add(g); renderSetup(); },
  'su-rename': renameDialog,
  'su-rename-go': startRename,
  'su-undo': startUndo,
  'su-ch-reset': el => { const c = SU.channels[+el.dataset.ch]; c.stain = c.orig.stain; c.color = c.orig.color; renderSetup(); },
  'su-save': saveSetup,
  'su-another': () => { Object.assign(SU, { labels: null, plan: null, stamp: null, renamed: 0, note: '' }); goStep('folder'); },
});

document.addEventListener('input', e => {
  if (!SU.open) return;
  const t = e.target;
  if (t.dataset.su === 'folder') { SU.folder = t.value; return; }
  if (t.dataset.su === 'dest') {
    SU.unpackDest = t.value.trim().replace(/^['"]|['"]$/g, '');
    const b = $('#setup [data-act="su-unpack"]');
    if (b) b.disabled = !SU.unpackDest || !SU.unpackSel.size;
    return;
  }
  if (t.dataset.ch != null) {
    const c = SU.channels[+t.dataset.ch];
    c[t.dataset.k] = t.value;
    if (t.dataset.k === 'color') t.closest('.su-swatch').style.setProperty('--c', t.value);
    const row = t.closest('.su-chan');
    row.querySelector('[data-act="su-ch-reset"]').hidden = c.stain === c.orig.stain && c.color === c.orig.color;
    const prev = $('#setup .su-preview');
    if (prev) prev.innerHTML = SU.channels.map(x => `<span style="--c:${esc(x.color)}">${esc(x.stain)}</span>`).join('');
    SU.dirty = true;
    return;
  }
  if (t.dataset.f && t.dataset.g != null && t.dataset.f !== 'kind') {
    const g = SU.labels.groups[+t.dataset.g], ei = +t.dataset.e, k = t.dataset.f;
    if (ei < 0) {
      g.shared[k] = t.value;
      g.guessed = (g.guessed || []).filter(x => x !== k);
      t.classList.remove('guess');
      t.removeAttribute('title');
      document.querySelectorAll(`#setup .su-group[data-g="${t.dataset.g}"] .su-row:not(.su-shared) input[data-f="${k}"]`).forEach(i => { i.placeholder = t.value; });
    } else {
      g.entries[ei].fields[k] = t.value;
    }
    SU.dirty = true;
    SU.channels = null;
    schedulePlan();
  }
});

document.addEventListener('change', e => {
  if (!SU.open) return;
  const t = e.target;
  if (t.dataset.suCzi) { t.checked ? SU.unpackSel.add(t.dataset.suCzi) : SU.unpackSel.delete(t.dataset.suCzi); renderSetup(); return; }
  if (t.dataset.suSkip != null) {
    SU.labels.groups[+t.dataset.suSkip].skip = t.checked;
    SU.dirty = true; SU.channels = null;
    renderSetup(); schedulePlan(0);
    return;
  }
  if (t.dataset.f === 'kind') {
    SU.labels.groups[+t.dataset.g].entries[+t.dataset.e].kind = t.value;
    SU.dirty = true; SU.channels = null;
    renderSetup(); schedulePlan(0);
  }
});

document.addEventListener('keydown', e => {
  if (!SU.open || !$('#modal').hidden) return;
  if (e.key === 'Escape') { hideTip(); return; }
  if (e.key === 'Enter' && e.target.dataset?.su === 'folder') { e.preventDefault(); startDetect(false); }
});
document.addEventListener('mouseover', e => { const b = e.target.closest?.('.su-tipbtn'); if (b) showTip(b); });
document.addEventListener('mouseout', e => { if (e.target.closest?.('.su-tipbtn')) hideTip(); });
document.addEventListener('focusin', e => { const b = e.target.closest?.('.su-tipbtn'); if (b) showTip(b); else hideTip(); });
document.addEventListener('scroll', hideTip, true);

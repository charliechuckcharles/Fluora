/* The intro: a short guided tour once a project's first images are ready, a few tips the first
   time an image is opened, and a Help page (the ? in the header) with how-tos and shortcuts.
   Each tour shows once per person; Help can replay it any time. */
'use strict';

const TOURS = {
  main: [
    { title: 'Your images are ready',
      body: 'This quick tour shows where everything is. It takes about a minute, and you can open it again from Help (the ? at the top right). Everything you do in Fluora saves by itself.',
      welcome: true },
    { target: '#grid .tile, #grid', title: 'Every image, grouped by slide',
      body: 'Click any image to open it big. That’s where you mark it, write notes and take snaps.' },
    { target: '#filters', title: 'Find the images you want',
      body: 'Show only the ones still to review, or one condition, stain or slide. The bar under each slide fills in as you mark its images.' },
    { target: '.logo-fl', title: 'Your project’s flower',
      body: 'It opens a little more with every image you mark, and is in full bloom when you’ve reviewed them all. Hover over it to see how far you’ve got.' },
    { target: '.tabs', title: 'Five ways to look',
      body: '<b>Slides</b> to review. <b>Gallery</b> shows your good and okay images side by side. <b>Board</b> lets you drag images into groups, e.g. for a figure. <b>Snaps</b> keeps the zoomed-in pictures you take. <b>Bouquet</b> shows off the very best ones you pick.' },
    { target: '[data-act="contrast"]', title: 'Make stains comparable',
      body: 'One brightness and contrast setting per stain, used for every image, so conditions can be compared fairly by eye. Your original files are never changed.' },
    { target: '#help-btn', title: 'Help is here',
      body: 'How-tos for everything, all the keyboard shortcuts, and this tour again.' },
  ],
  viewer: [
    { target: '#viewer .marks', title: 'Mark it and move on',
      body: 'Press <kbd>G</kbd> good, <kbd>O</kbd> okay or <kbd>X</kbd> not good. It saves straight away, with your name, and goes to the next image. Use <kbd>←</kbd> <kbd>→</kbd> to move without marking.' },
    { target: '#viewer textarea[data-in="note"]', title: 'Notes save as you type',
      body: 'Anything worth remembering about this image. Notes show up with the image everywhere, and can be filtered on.' },
    { target: '#viewer .snapbtn', title: 'Snap what you see',
      body: 'Scroll to zoom and drag to move. Then press <kbd>S</kbd>: Fluora saves exactly that view, sharp from the full-resolution file, with a scale bar, in the Snaps tab.' },
    { target: '#viewer .pickbtn', title: 'Pick the very best',
      body: 'Click the flower, or press <kbd>P</kbd>, to pick an image or a snap for your <b>Bouquet</b>: a showcase of the best of the project.' },
    { target: '#viewer .channels', title: 'One stain at a time',
      body: 'Look at each stain on its own in black and white, or all of them merged. Keys <kbd>1</kbd> <kbd>2</kbd> <kbd>3</kbd>… and <kbd>M</kbd> for merged.' },
  ],
};

const TOUR = { name: null, i: 0 };
const seenTours = () => S.data.profile?.seen || {};

function markSeen(name) {
  S.data.profile = S.data.profile || {};
  S.data.profile.seen = { ...seenTours(), [name]: true };
  api('/api/profile', { seen: { [name]: true } }).catch(() => {});
}

function startTour(name) {
  closeModal();
  TOUR.name = name;
  TOUR.i = 0;
  if (name === 'main' && typeof showTab === 'function') showTab('slides');
  showStep();
}

function endTour() {
  if (TOUR.name) markSeen(TOUR.name);
  TOUR.name = null;
  $('#tour').hidden = true;
  $('#tour').innerHTML = '';
}

function stepTarget(step) {
  if (!step.target) return null;
  for (const sel of step.target.split(',').map(x => x.trim())) {
    const el = document.querySelector(sel);
    if (el && el.offsetParent !== null) {
      const r = el.getBoundingClientRect();
      if (r.width && r.height) return el;
    }
  }
  return null;
}

function showStep() {
  const steps = TOURS[TOUR.name];
  if (!steps) return;
  let step = steps[TOUR.i];
  // skip a step whose part of the screen isn't showing
  while (step && step.target && !stepTarget(step)) { TOUR.i++; step = steps[TOUR.i]; }
  if (!step) { endTour(); return; }
  const n = steps.length, i = TOUR.i;
  const el = stepTarget(step);
  const root = $('#tour');
  root.hidden = false;
  root.innerHTML = `${el ? '<div class="tour-hole"></div>' : '<div class="tour-dim"></div>'}
    <div class="tour-card ${step.welcome ? 'welcome' : ''}" role="dialog" aria-modal="true" aria-labelledby="tour-title">
      ${step.welcome ? `<div class="tour-fl">${flower(1, { size: 44, bloom: true })}</div>` : ''}
      <div class="tour-count">${step.welcome ? 'Welcome' : steps[0].welcome ? `${i} of ${n - 1}` : `${i + 1} of ${n}`}</div>
      <h3 id="tour-title">${step.title}</h3>
      <p>${step.body}</p>
      <div class="tour-foot">
        ${step.welcome
          ? '<button class="ghost" data-act="tour-skip">Not now</button><button class="primary" data-act="tour-next">Show me around</button>'
          : `<button class="ghost" data-act="tour-skip">Skip tour</button><span class="tour-gap"></span>
             ${i > (steps[0].welcome ? 1 : 0) ? '<button data-act="tour-back">Back</button>' : ''}
             <button class="primary" data-act="tour-next">${i === n - 1 ? 'Done' : 'Next'}</button>`}
      </div>
    </div>`;
  if (step.welcome) bloomFlowers(root);
  placeStep(el);
  root.querySelector('.tour-card .primary')?.focus();
}

// Puts the highlight round the target and the card next to it, inside the window
function placeStep(el) {
  const card = $('#tour .tour-card'), hole = $('#tour .tour-hole');
  if (!card) return;
  if (!el) {
    card.style.left = `${Math.round((innerWidth - card.offsetWidth) / 2)}px`;
    card.style.top = `${Math.round((innerHeight - card.offsetHeight) / 2.4)}px`;
    return;
  }
  const r = el.getBoundingClientRect(), pad = 6;
  const box = { x: Math.max(4, r.left - pad), y: Math.max(4, r.top - pad) };
  box.w = Math.min(innerWidth - 8, r.right + pad) - box.x;
  box.h = Math.min(innerHeight - 8, r.bottom + pad) - box.y;
  Object.assign(hole.style, { left: `${box.x}px`, top: `${box.y}px`, width: `${box.w}px`, height: `${box.h}px` });
  const cw = card.offsetWidth, ch = card.offsetHeight, gap = 14;
  // below, else above, else to the side with the most room
  let x = box.x + box.w / 2 - cw / 2, y = box.y + box.h + gap;
  if (y + ch > innerHeight - 12) y = box.y - ch - gap;
  if (y < 12) {
    y = Math.min(Math.max(12, box.y + box.h / 2 - ch / 2), innerHeight - ch - 12);
    x = box.x + box.w + gap + cw < innerWidth ? box.x + box.w + gap : box.x - cw - gap;
  }
  card.style.left = `${Math.round(Math.min(Math.max(12, x), innerWidth - cw - 12))}px`;
  card.style.top = `${Math.round(Math.min(Math.max(12, y), innerHeight - ch - 12))}px`;
}

// The main tour starts once, the first time a project has images and nothing else is open
function maybeStartTour() {
  if (TOUR.name || seenTours().main || !S.data.slices?.length) return;
  if (!$('#modal').hidden || (typeof SU !== 'undefined' && SU.open) || S.v.open) return;
  setTimeout(() => {
    if (!TOUR.name && !seenTours().main && $('#modal').hidden && !S.v.open && !(typeof SU !== 'undefined' && SU.open)) startTour('main');
  }, 600);
}

// The viewer tips, the first time an image is opened
function maybeViewerTips() {
  if (TOUR.name || seenTours().viewer) return;
  setTimeout(() => { if (S.v.open && !TOUR.name && !seenTours().viewer) startTour('viewer'); }, 450);
}

// ---------------------------------------------------------------- Help

const KEYS = [
  ['← →', 'Previous / next image'], ['G  O  X', 'Mark good, okay, not good, and go to the next'], ['1 – 9', 'Show one stain'],
  ['M', 'Merged (all stains)'], ['S', 'Snap what you see'], ['P', 'Pick for your Bouquet'], ['B', 'Add to the board'], ['F', 'Open full resolution in Fiji'],
  ['⇧ F', 'Open each channel in Fiji'], ['Scroll', 'Zoom in and out'], ['Double-click', 'Fit the image to the window'], ['Esc', 'Close'],
];

function helpDialog() {
  const sec = (title, items) => `<section class="help-sec"><h3>${title}</h3><ul>${items.map(x => `<li>${x}</li>`).join('')}</ul></section>`;
  showModal(`<div class="help">
    <div class="help-head"><div>${flower(1, { size: 30 })}</div><div><h2>How to use Fluora</h2>
      <p>Review your microscopy images, mark the good ones, and keep sharp, labelled snaps. Everything you do stays on this computer.</p></div></div>
    <div class="help-actions"><button class="primary" data-act="tour-main">Take the tour again</button>
      <button data-act="tour-viewer" ${S.data.slices?.length ? '' : 'disabled'}>Show the image tips again</button></div>
    <div class="help-grid">
      ${sec('Getting started', [
        '<b>Projects</b> are separate sets of images, each with their own marks, notes, board and snaps. Switch or make one from the menu under the app’s name.',
        '<b>Settings → Folder, labels &amp; channels</b> chooses the image folder, labels the images and names the stains. CZIs can be unpacked there with Fiji.',
        '<b>Refresh</b> looks for images added to the folder since; new ones are highlighted for you to label.',
      ])}
      ${sec('Reviewing', [
        'Click an image to open it. Mark it <b>good</b>, <b>okay</b> or <b>not good</b>; Fluora moves on to the next.',
        'Notes save as you type, labelled with your name and the date.',
        'Use the list on the left to show only what’s still to review, or one condition, stain or slide.',
        'The flower opens as you mark images: full bloom means every image has been reviewed.',
      ])}
      ${sec('Looking closer', [
        'Scroll to zoom and drag to move. Pick a stain to see it on its own in black and white.',
        '<b>Screen brightness</b> only changes your view. <b>Brightness &amp; contrast</b> changes the saved previews for a whole stain, so every image is shown the same way.',
        '<b>Open in Fiji</b> opens the original full-resolution file (on the lab drive this needs the VPN).',
      ])}
      ${sec('Snaps', [
        'Zoom in on something worth keeping and press <b>S</b>. The snap is made from the full-resolution file, with a scale bar.',
        'Each snap remembers its slide, image, stain and spot, also inside the PNG file, so it can always be traced back.',
        'The <b>Snaps</b> tab lists them all: add a caption, go back to the exact spot, or put it on the board.',
      ])}
      ${sec('Bouquet', [
        'Pick the images and snaps you love with the flower button, or <b>P</b>. A small flower marks them everywhere.',
        'The <b>Bouquet</b> tab gathers them into one showcase. Click one to see it full screen, step through with ← →, and jump back to the image.',
      ])}
      ${sec('Saving', [
        '<b>Everything saves by itself</b>, as you go: marks the moment you click, notes as you type, the board and snap captions as you change them. There’s no Save button to remember.',
        'The one exception is the setup (folder, labels and channels): it’s saved when you press <b>Save and make previews</b>.',
        'It’s all kept on this computer, separately for each project.',
      ])}
      ${sec('Board and lists', [
        '<b>Board</b>: drag images and snaps from the left, make groups (double-click empty space), and copy a group’s list for your notes.',
        '<b>Gallery</b>: your good and okay images side by side, by stain.',
        '<b>Good snap list</b>: every image marked good, by slide, ready to copy.',
      ])}
      <section class="help-sec"><h3>Keyboard shortcuts <span>in the image viewer</span></h3>
        <dl class="help-keys">${KEYS.map(([k, d]) => `<dt>${k.split('  ').map(x => `<kbd>${x}</kbd>`).join(' ')}</dt><dd>${d}</dd>`).join('')}</dl></section>
    </div>
    <p class="help-fine">Your images are only ever read, unless you choose to rename them in Labels. Marks, notes, snaps and the board are saved on this computer, under your name.</p>
    <div class="foot"><button class="primary" data-act="close-modal">Close</button></div>
  </div>`);
  bloomFlowers($('#modal'));
}

Object.assign(ACT, {
  help: helpDialog,
  'tour-next': () => { TOUR.i++; showStep(); },
  'tour-back': () => { TOUR.i = Math.max(0, TOUR.i - 1); showStep(); },
  'tour-skip': endTour,
  'tour-main': () => startTour('main'),
  'tour-viewer': () => {
    closeModal();
    if (!S.v.open) openViewer((typeof filtered === 'function' && filtered()[0] || S.data.slices[0]).id);
    setTimeout(() => startTour('viewer'), 300);
  },
});

document.addEventListener('keydown', e => {
  if (!TOUR.name) return;
  if (e.key === 'Escape') { e.preventDefault(); e.stopImmediatePropagation(); endTour(); }
  else if (e.key === 'ArrowRight' || e.key === 'Enter') { e.preventDefault(); e.stopImmediatePropagation(); ACT['tour-next'](); }
  else if (e.key === 'ArrowLeft') { e.preventDefault(); e.stopImmediatePropagation(); ACT['tour-back'](); }
  else { e.stopImmediatePropagation(); }
}, true);
addEventListener('resize', () => { if (TOUR.name) showStep(); });

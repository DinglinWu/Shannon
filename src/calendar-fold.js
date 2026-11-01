// One two-sided leaf: the conversation turns toward the viewer and its reverse
// becomes the compact answer. Both faces share the same moving hinge.
const paintedFrame = () => new Promise(resolve =>
  requestAnimationFrame(() => requestAnimationFrame(resolve)));

// A native transparent-window resize must follow a painted final layout.
// Keep the compositing hints until the resized WebView has painted as well.
export async function settleCalendarFold({ commit, resize, release, paint = paintedFrame }) {
  try {
    commit();
    await paint();
    await resize();
    await paint();
  } finally {
    release();
  }
}

export function prepareCalendarFront(chat) {
  const viewport = chat.getBoundingClientRect();
  const front = document.createElement('div');
  front.className = 'calendar-front';
  front.setAttribute('aria-hidden', 'true');
  front.inert = true;
  front.style.width = `${viewport.width}px`;
  front.style.height = `${viewport.height}px`;
  // Freeze only visible rows. Moving the live scroll container through a 3D
  // tree invalidates its paint/scroll layers and content-visibility caches.
  // Read all geometry first, then build the detached, non-scrolling face.
  const rows = Array.from(chat.children, row => ({ row, rect: row.getBoundingClientRect() }))
    .filter(({ rect }) => rect.bottom > viewport.top && rect.top < viewport.bottom);
  for (const { row, rect } of rows) {
    const copy = row.cloneNode(true);
    copy.removeAttribute('id');
    for (const element of copy.querySelectorAll('[id]')) element.removeAttribute('id');
    Object.assign(copy.style, {
      position: 'absolute', margin: '0', contentVisibility: 'visible',
      left: `${rect.left - viewport.left}px`, top: `${rect.top - viewport.top}px`,
      width: `${rect.width}px`, height: `${rect.height}px`,
    });
    front.append(copy);
  }
  return front;
}

export async function calendarFold({ main, chat, front, result, dock, height, offsetX, dockY, sourceWidth, width, duration }) {
  const scroll = {top:result.scrollTop,left:result.scrollLeft};
  const restoreScroll=()=>{result.scrollTop=scroll.top;result.scrollLeft=scroll.left;};
  const resultSlot = document.createComment('calendar-back');
  result.before(resultSlot);
  const leaf = document.createElement('div');
  leaf.className = 'calendar-leaf';
  leaf.style.height = `${height}px`;
  const back = document.createElement('div');
  back.className = 'calendar-back';
  // The hinge follows the input's TOP edge. Offset the reverse face so the
  // compact answer still lands below the input, without moving that hinge.
  back.style.transform = `rotateX(180deg) translateY(${dock.offsetHeight}px)`;
  const shade = document.createElement('div');
  shade.className = 'calendar-shade';
  shade.style.width = front.style.width;
  leaf.append(front, shade, back);
  back.append(result);
  main.append(leaf);
  restoreScroll();
  const motions = [];
  try {
    chat.classList.add('hidden');
    // The target is painted only on the reverse; never reveal it underneath
    // the turning page. Keep the original scroll containers and their state.
    result.style.visibility = '';
    const endY = -height;
    const timing = { duration, easing: 'cubic-bezier(0.32, 0.12, 0.32, 1)', fill: 'forwards' };
    motions.push(leaf.animate([
      { transform: `translate(${offsetX}px, ${dockY - height}px) perspective(1800px) rotateX(0deg)` },
      { transform: `translate(0, ${endY}px) perspective(1800px) rotateX(-180deg)` },
    ], timing));
    motions.push(dock.animate([
      { transform: `translateY(${dockY}px) scaleX(${sourceWidth / width})` },
      { transform: 'translateY(0) scaleX(1)' },
    ], timing));
    motions.push(shade.animate([
      { opacity: 0, offset: 0 },
      { opacity: 0.2, offset: 0.5 },
      { opacity: 0, offset: 1 },
    ], timing));
    for (const motion of motions) motion.pause();
    // Give WebView one paint to rasterize both faces before the compositor
    // starts rotating. No layout, Markdown parsing or native resize per frame.
    if (duration) await paintedFrame();
    const startTime = document.timeline.currentTime;
    for (const motion of motions) {
      motion.play();
      motion.startTime = startTime;
    }
    await Promise.all(motions.map(motion => motion.finished));
  } finally {
    chat.classList.add('hidden');
    resultSlot.replaceWith(result);
    restoreScroll();
    leaf.remove();
    for (const motion of motions) motion.cancel();
  }
}

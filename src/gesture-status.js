import { invoke } from './invoke.js';
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
import { icon, statusView, trackingHints } from './gesture-status-model.js';

const demo = new URLSearchParams(location.search).get('demo');
const win = demo ? null : getCurrentWindow();
const island = document.getElementById('gesture-island');
const details = document.getElementById('gesture-details');
const stop = document.getElementById('gesture-stop');
const error = document.getElementById('gesture-error');
const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;
let state = { phase: 'starting' };
let busy = false;
let hovered = false;
let keyboard = false;
let resizeVersion = 0;
let shrinkTimer;
let exitMotion;
let lastAnnouncement = '';
let lastActionId;

stop.querySelector('.button-icon').innerHTML = icon('close');

function render() {
  const view = statusView(state);
  if (exitMotion && state.phase !== 'stopped') { exitMotion.cancel(); exitMotion = null; }
  island.dataset.tone = view.tone;
  const symbol = document.getElementById('gesture-glyph');
  if (symbol.dataset.glyph !== view.glyph) { symbol.innerHTML = icon(view.glyph); symbol.dataset.glyph = view.glyph; }
  const actionId = state.last_action?.at;
  if (view.tone === 'action' && actionId !== lastActionId) {
    lastActionId = actionId;
    symbol.classList.remove('arrived');
    if (!reduceMotion) void symbol.offsetWidth; // Restart the one-shot animation.
    symbol.classList.add('arrived');
  }
  const elapsed = Number.isFinite(state.updated_at) ? Math.max(0, Date.now() / 1000 - state.updated_at) : 0;
  const remaining = Math.max(0, view.remaining - elapsed);
  island.style.setProperty('--recognition-progress', Math.max(0, Math.min(1, remaining / (state.effective_config?.recognition_seconds || 6))));
  const description = [view.title, view.subtitle,
    view.active && remaining > 0 ? `剩余 ${Math.ceil(remaining)} 秒` : ''].filter(Boolean).join('，');
  island.title = error.hidden ? description : error.textContent;
  island.setAttribute('aria-label', description);
  const tracking = state.tracking || '';
  let hint = view.subtitle || '捏合后松开：播放 / 暂停';
  if (!view.active) hint = '张掌面向摄像头，保持片刻唤醒';
  else if (view.tone === 'action') hint = '已识别并执行 · ' + view.title;
  else if (trackingHints[tracking]) hint = trackingHints[tracking];
  else if (tracking.startsWith('PINCH')) hint = '已识别捏合，松开手指确认';
  else if (tracking.endsWith('WRIST')) hint = '左背上一曲 · 右掌下一曲 · 上下滚动';
  else if (tracking === 'RETURN WRIST TO CENTER') hint = '请将手腕移回中间，再摆手';
  else if (tracking.endsWith('VOLUME')) hint = '伸出食指和中指，转动手腕调音量';
  else if (tracking.endsWith('TRACKS')) hint = '左摆手背向前：上一曲；右摆手心向前：下一曲';
  else if (tracking === 'COOLDOWN') hint = '稍候片刻，再做下一个手势';
  else if (tracking === 'NO HAND') hint = '请将手移回摄像头画面';
  else if (tracking === 'OUTSIDE CONTROL AREA') hint = '请将手掌移入画面中央';
  if (view.tone === 'error') hint = view.subtitle || '请检查摄像头与识别服务';
  document.getElementById('gesture-title').textContent = view.title;
  document.getElementById('gesture-hint').textContent = hint;
  document.getElementById('gesture-hint').title = hint;
  document.getElementById('gesture-device').textContent = `${view.hand} · ${view.fps} · ${view.camera}`;

  stop.disabled = busy;
  const announcement = [view.title, view.subtitle].filter(Boolean).join('，');
  if (announcement !== lastAnnouncement) {
    document.getElementById('gesture-announcement').textContent = announcement;
    lastAnnouncement = announcement;
  }
  document.title = `${view.title} · 闪念手势`;
}

async function expand() {
  hovered = island.matches(':hover');
  clearTimeout(shrinkTimer);
  const version = ++resizeVersion;
  try {
    if (!demo) await win.setSize(new LogicalSize(340, 130));
    if (version !== resizeVersion) return;
    island.classList.add('expanded');
    details.inert = false;
  } catch (err) { showError(err); }
}
function collapse() {
  if (hovered || (keyboard && island.contains(document.activeElement))) return;
  const version = ++resizeVersion;
  island.classList.remove('expanded');
  details.inert = true;
  clearTimeout(shrinkTimer);
  shrinkTimer = setTimeout(async () => {
    if (version !== resizeVersion || demo) return;
    try { await win.setSize(new LogicalSize(340, 86)); } catch (err) { showError(err); }
  }, reduceMotion ? 0 : 240);
}
function showError(err) {
  error.hidden = false;
  error.textContent = String(err);
  island.dataset.error = 'true';
  island.title = String(err);
  document.getElementById('gesture-glyph').innerHTML = icon('alert');
}
async function control(action) {
  if (busy) return;
  busy = true;
  error.hidden = true;
  delete island.dataset.error;
  render();
  try {
    if (demo) {
      state = { phase: 'stopped' };
    } else {
      await invoke('gesture_overlay_control', { action });
      state = await invoke('gesture_overlay_snapshot');
    }
  } catch (err) { showError(err); }
  finally { busy = false; render(); }
}
stop.addEventListener('click', () => control('stop'));
island.addEventListener('mouseenter', () => { hovered = true; expand(); });
island.addEventListener('mouseleave', () => { hovered = false; collapse(); });
island.addEventListener('focusin', expand);
island.addEventListener('focusout', () => setTimeout(collapse, 0));
document.addEventListener('pointerdown', () => { keyboard = false; }, true);
document.addEventListener('keydown', event => {
  if (event.key === 'Tab') keyboard = true;
  if (event.key === 'Escape') { keyboard = false; island.blur(); collapse(); }
});

async function main() {
  if (!demo) await win.setSize(new LogicalSize(340, 86));
  if (demo) {
    const base = { phase: 'running', process_running: true, recognition_active: true, active_remaining: 6, mode: 'shortcuts', hand: 'Right', camera_name: 'Integrated Camera', fps: 29, bindings: { pinch: 'play_pause' } };
    const examples = {
      ready: { tracking: 'READY' }, waiting: { tracking: 'NO HAND' }, dormant: { recognition_active: false }, waking: { tracking: 'CONFIRMING PALM', recognition_active: false },
      pinch: { tracking: 'PINCH - release to confirm' }, scroll: { tracking: 'SCROLL' },
      action: { tracking: 'COOLDOWN', last_action: { gesture: 'swipe_right', action: 'next_track', at: Date.now() / 1000 } },
      error: { phase: 'error', error: '无法打开摄像头，请检查设备占用情况' },
    };
    state = { ...base, ...examples[demo] };
    if (new URLSearchParams(location.search).has('expanded')) { island.classList.add('expanded'); details.inert = false; }
  } else {
    const unlisten = await win.listen('gesture-status', ({ payload }) => { state = payload; render(); });
    const unexit = await win.listen('gesture-exit', () => {
      state = { ...state, phase: 'stopped' };
      ++resizeVersion;
      clearTimeout(shrinkTimer);
      details.inert = true;
      exitMotion = island.animate([
        { clipPath: 'inset(0 0 0 0 round 0 0 20px 20px)', opacity: 1 },
        { clipPath: 'inset(0 50% 100% 50% round 16px)', transform: 'translateY(-8px)', opacity: 0 },
      ], { duration: reduceMotion ? 0 : 280, fill: 'forwards', easing: 'cubic-bezier(.4,0,.6,1)' });
    });
    window.addEventListener('beforeunload', () => { unlisten(); unexit(); clearInterval(refresh); }, { once: true });
    state = await invoke('gesture_overlay_snapshot');
  }
  render();
  if (!reduceMotion) island.animate([
    { clipPath: 'inset(0 48% 88% 48% round 20px)', transform: 'translateY(-5px)' },
    { clipPath: 'inset(0 0 0 0 round 0 0 20px 20px)', transform: 'translateY(0)' },
  ], { duration: 380, easing: 'cubic-bezier(.22,1,.36,1)' });
}
const refresh = setInterval(render, 200);
main().catch(showError);

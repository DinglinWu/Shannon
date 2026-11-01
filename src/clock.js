import { invoke } from "./invoke.js";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { Clock, parseDuration, formatTime } from "./clock-model.js";
import { modeForArrow } from "./clock-keyboard.js";

const win = getCurrentWindow();
const setup = document.getElementById("setup");
const notch = document.getElementById("notch");
const form = document.getElementById("clock-form");
const durationInput = document.getElementById("duration");
const error = document.getElementById("error");
const pause = document.getElementById("pause");
const controls = document.getElementById("controls");
const label = document.getElementById("mode-label");
const time = document.getElementById("time");
const announcement = document.getElementById("announcement");
function setPauseAction(action) {
  const name = { pause: "暂停", resume: "继续", restart: "重新开始" }[action];
  pause.dataset.action = action;
  pause.title = name;
  pause.setAttribute("aria-label", name);
}
let clock = null;
let finished = false;
let hoverVersion = 0;
let collapseTimer;
let hovered = false;
let keyboardNavigation = false;
let docking = false;
let closing = false;

const params = new URLSearchParams(location.search);
if (params.get("mode") === "up") form.elements.mode.value = "up";
const preset = Number(params.get("seconds"));
if (preset > 0 && preset <= 86400) durationInput.value = `${preset}s`;

function updateMode() {
  const up = form.elements.mode.value === "up";
  form.classList.toggle("mode-up", up);
  for (const [id, inactive] of [["duration-field", up]]) {
    const panel = document.getElementById(id);
    panel.inert = inactive;
    panel.setAttribute("aria-hidden", String(inactive));
  }
  error.textContent = "";
}
form.addEventListener("change", updateMode);
updateMode();

function focusSetup() {
  if (clock || docking || closing) return;
  if (form.elements.mode.value === "down") {
    durationInput.focus({ preventScroll: true });
    durationInput.select();
  } else {
    document.getElementById("start").focus({ preventScroll: true });
  }
}
// 初次加载与再次唤回窗口都能直接输入；不需要先点击窗口或时长。
requestAnimationFrame(focusSetup);
win.onFocusChanged(({ payload }) => {
  if (payload) focusSetup();
}).then((unlisten) => {
  window.addEventListener("beforeunload", unlisten, { once: true });
}).catch((err) => console.warn("计时器焦点监听不可用", err));

function render() {
  if (!clock) return;
  const remaining = clock.remaining();
  if (clock.mode === "down" && remaining === 0 && !finished) {
    clock.pause();
    finished = true;
    notch.classList.add("done");
    setPauseAction("restart");
    label.textContent = "时间到";
    announcement.textContent = "倒计时结束";
  }
  time.textContent = formatTime(clock.mode === "down" ? remaining : clock.elapsedAt(), clock.mode === "down");
  document.title = `${time.textContent} · 闪念计时器`;
}
setInterval(render, 100);

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  if (docking || closing) return;
  const mode = form.elements.mode.value;
  const duration = mode === "down" ? parseDuration(durationInput.value) : 0;
  if (mode === "down" && duration === null) {
    error.textContent = "请输入 1 秒～24 小时的时长，例如 5m、90s、01:30。";
    durationInput.focus();
    return;
  }
  const start = document.getElementById("start");
  start.disabled = true;
  docking = true;
  document.body.classList.add("docking");
  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const motions = [];
  try {
    // 设置面板退出后，在屏幕顶部直接从中间展开整块岛体。
    const dismiss = setup.animate([
      { opacity: 1, transform: "scale(1)" },
      { opacity: 0, transform: "scale(.98)" },
    ], { duration: reduceMotion ? 0 : 100, easing: "ease-out", fill: "forwards" });
    motions.push(dismiss);
    await dismiss.finished;
    setup.hidden = true;
    await invoke("dock_clock");
    clock = new Clock(mode, duration);
    clock.resume();
    notch.hidden = false;
    label.textContent = mode === "up" ? "正计时" : "倒计时";
    render();
    const reveal = notch.animate([
      { clipPath: "inset(0 48% 88% 48% round 20px)", transform: "translateY(-5px)" },
      { clipPath: "inset(0 0 0 0 round 0 0 20px 20px)", transform: "translateY(0)" },
    ], { duration: reduceMotion ? 0 : 380, easing: "cubic-bezier(.22,1,.36,1)", fill: "both" });
    const content = document.getElementById("timer-line").animate([
      { opacity: 0, transform: "translateY(-4px)" },
      { opacity: 1, transform: "translateY(0)" },
    ], { delay: reduceMotion ? 0 : 100, duration: reduceMotion ? 0 : 220, fill: "both" });
    motions.push(reveal, content);
    await Promise.all([reveal.finished, content.finished]);
    motions.forEach((motion) => motion.cancel());
    announcement.textContent = "计时已开始，鼠标悬停可暂停或关闭";
  } catch (err) {
    motions.forEach((motion) => motion.cancel());
    clock = null;
    finished = false;
    setup.hidden = false;
    notch.hidden = true;
    notch.classList.remove("done", "paused");
    setPauseAction("pause");
    error.textContent = "无法吸附到屏幕顶部：" + String(err);
  } finally {
    docking = false;
    document.body.classList.remove("docking");
    hovered = notch.matches(":hover");
    if (clock && hovered) expand();
    if (!clock) focusSetup();
    start.disabled = false;
  }
});

function restartClock() {
  if (!clock || docking || closing) return;
  clock = new Clock(clock.mode, clock.duration);
  clock.resume();
  finished = false;
  notch.classList.remove("done", "paused");
  setPauseAction("pause");
  label.textContent = clock.mode === "up" ? "正计时" : "倒计时";
  announcement.textContent = "已重新开始计时";
  render();
}
document.getElementById("restart").addEventListener("click", restartClock);
pause.addEventListener("click", () => {
  if (!clock || closing) return;
  if (finished) { restartClock(); return; }
  if (clock.startedAt === null) {
    clock.resume();
    setPauseAction("pause");
    label.textContent = clock.mode === "up" ? "正计时" : "倒计时";
    notch.classList.remove("paused");
    announcement.textContent = "继续计时";
  } else {
    clock.pause();
    setPauseAction("resume");
    label.textContent = "已暂停";
    notch.classList.add("paused");
    announcement.textContent = "计时已暂停";
  }
  render();
});

function reportError(err) {
  const el = document.getElementById("notch-error");
  el.hidden = false;
  el.textContent = String(err);
}

async function expand() {
  if (docking || closing || !clock) return;
  clearTimeout(collapseTimer);
  const version = ++hoverVersion;
  try {
    await win.setSize(new LogicalSize(260, 110));
    if (version !== hoverVersion) return;
    notch.classList.add("expanded");
    controls.inert = false;
  } catch (err) { reportError(err); }
}
function collapse() {
  if (docking || closing || !clock) return;
  if (hovered || (keyboardNavigation && notch.contains(document.activeElement))) return;
  const version = ++hoverVersion;
  notch.classList.remove("expanded");
  controls.inert = true;
  clearTimeout(collapseTimer);
  collapseTimer = setTimeout(async () => {
    if (version !== hoverVersion) return;
    try { await win.setSize(new LogicalSize(260, 54)); }
    catch (err) { reportError(err); }
  }, 240);
}
notch.addEventListener("mouseenter", () => { hovered = true; expand(); });
notch.addEventListener("mouseleave", () => { hovered = false; collapse(); });
// 键盘用户可先点击顶部时间行聚焦，再用 Tab 展开操作。
notch.tabIndex = 0;
notch.addEventListener("focusin", expand);
notch.addEventListener("focusout", () => setTimeout(collapse, 0));
document.addEventListener("pointerdown", () => { keyboardNavigation = false; }, true);
document.addEventListener("keydown", (event) => {
  if (event.key === "Tab") keyboardNavigation = true;
});
async function closeClock(afterExit) {
  if (closing || docking) return;
  closing = true;
  // 阻止悬停与延迟收起在退出中调整原生窗口，保留完整的动画画布。
  ++hoverVersion;
  clearTimeout(collapseTimer);
  document.body.classList.add("closing");
  controls.inert = true;
  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const surface = clock ? notch : setup;
  const motion = surface.animate(clock ? [
    { clipPath: "inset(0 0 0 0 round 0 0 20px 20px)", transform: "translateY(0)", opacity: 1 },
    { clipPath: "inset(0 34% 74% 34% round 16px)", transform: "translateY(-3px)", opacity: 1, offset: 0.7 },
    { clipPath: "inset(0 50% 100% 50% round 16px)", transform: "translateY(-8px)", opacity: 0 },
  ] : [
    { opacity: 1, transform: "translateY(0) scale(1)" },
    { opacity: 0, transform: "translateY(-8px) scale(.96)" },
  ], {
    duration: reduceMotion ? 0 : clock ? 280 : 180,
    easing: "cubic-bezier(.4,0,.6,1)",
    fill: "forwards",
  });
  try {
    // 先收聚整块岛体，画面完全消失后才销毁窗口。
    await motion.finished;
    if (typeof afterExit === "function") {
      await afterExit();
      motion.cancel();
      closing = false;
      document.body.classList.remove("closing");
      focusSetup();
    } else {
      await invoke("close_clock");
    }
  } catch (err) {
    motion.cancel();
    closing = false;
    document.body.classList.remove("closing");
    controls.inert = !notch.classList.contains("expanded");
    if (clock) reportError(err);
    else error.textContent = String(err);
    collapse();
  }
}
document.querySelectorAll(".close").forEach((button) => button.addEventListener("click", closeClock));

win.listen("clock-restart", ({ payload }) => closeClock(async () => {
  await invoke("reset_clock");
  clock = null;
  finished = false;
  notch.hidden = true;
  notch.classList.remove("done", "paused", "expanded");
  controls.inert = true;
  document.getElementById("notch-error").hidden = true;
  setup.hidden = false;
  form.elements.mode.value = payload.mode === "up" ? "up" : "down";
  durationInput.value = payload.seconds > 0 ? `${payload.seconds}s` : "25";
  setPauseAction("pause");
  announcement.textContent = "";
  updateMode();
})).then((unlisten) => {
  window.addEventListener("beforeunload", unlisten, { once: true });
}).catch((err) => console.warn("计时器重设监听不可用", err));
document.addEventListener("keydown", (event) => {
  if (event.isComposing || clock || docking || closing) return;
  if (event.key === "Escape") {
    event.preventDefault();
    closeClock();
    return;
  }
  // 无需修饰键：输入中先移动光标，到最右边后再按右键才进入正计时。
  if (!event.altKey && !event.shiftKey && !event.ctrlKey && !event.metaKey &&
      (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
    if (event.repeat) return;
    const selection = event.target === durationInput ? {
      start: durationInput.selectionStart, end: durationInput.selectionEnd, length: durationInput.value.length,
    } : null;
    const mode = modeForArrow(form.elements.mode.value, event.key, selection);
    if (mode) {
      event.preventDefault();
      form.elements.mode.value = mode;
      updateMode();
      keyboardNavigation = true;
      focusSetup();
      return;
    }
    // 单选框到达首尾时停住，避免浏览器默认的环绕切换。
    if (event.target instanceof Element && event.target.matches('input[name="mode"]')) event.preventDefault();
  }
  if (event.key === "Enter" && !event.altKey && !event.ctrlKey && !event.metaKey) {
    // 关闭按钮仍使用自己的 Enter 行为，其余设置位置均可回车开始。
    if (event.target instanceof Element && event.target.closest(".close")) return;
    event.preventDefault();
    if (!event.repeat) {
      keyboardNavigation = true;
      form.requestSubmit(document.getElementById("start"));
    }
  }
});

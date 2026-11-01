export function createLongPress({ trigger, release = () => {}, schedule = setTimeout, cancel = clearTimeout }) {
  let timer;
  let origin;
  let active = false;
  function stop() {
    cancel(timer); timer = undefined; origin = undefined;
    if (active) { active = false; release(); }
  }
  return {
    stop,
    leave() { if (!active) stop(); },
    up(event) {
      if (origin && event.pointerId === origin.id && (event.type === "pointercancel" || event.button === 0)) stop();
    },
    down(event) {
      if (event.button !== 0 || event.ctrlKey || event.altKey || event.shiftKey || event.metaKey || event.isPrimary === false) return;
      stop();
      origin = { x: event.clientX, y: event.clientY, id: event.pointerId };
      timer = schedule(() => { timer = undefined; active = true; trigger(origin.id); }, 650);
    },
    move(event) {
      if (!origin || event.pointerId !== origin.id) return;
      if (!(event.buttons & 1) || (!active && Math.hypot(event.clientX - origin.x, event.clientY - origin.y) > 8)) stop();
    },
  };
}

export function createVoiceDraft(value, start, end = start) {
  let before = value.slice(0, start);
  const after = value.slice(end);
  let expected = value;
  let caret = start;
  return {
    append(text, current, cursor) {
      if (current !== expected || cursor !== caret) return null;
      const words = text.trim();
      if (!words) return { value: expected, caret };
      if (/[a-z0-9]$/i.test(before) && /^[a-z0-9]/i.test(words)) before += " ";
      before += words;
      caret = before.length;
      expected = before + after;
      return { value: expected, caret };
    },
  };
}

export function initVoiceInput(input, invoke, listen, enabled=()=>true) {
  let session = null;
  let ready = false;
  let composing = false;
  let statusTimer;
  const host = input.parentNode;
  const status = document.createElement("span");
  status.className = "voice-input-status";
  status.setAttribute("role", "status");
  const ripple = document.createElement("div");
  ripple.className = "voice-ripples";
  ripple.setAttribute("aria-hidden", "true");
  for (let i = 0; i < 3; i++) ripple.append(document.createElement("i"));
  host.append(ripple, status);
  function show(text, temporary = false) {
    clearTimeout(statusTimer);
    status.textContent = text;
    if (temporary) statusTimer = setTimeout(() => { status.textContent = ""; }, 5000);
  }
  function wave(active) { host.classList.toggle("voice-listening", active); }
  async function stop(discard = false) {
    if (!session) return;
    if (discard) session.discard = true;
    if (session.stopping) return;
    const current = session;
    current.stopping = true;
    wave(false);
    show("正在结束语音输入…");
    try {
      // A release during model startup must stop that same session, not race its start command.
      await current.starting;
      if (session === current) await invoke("stop_voice_input", { id: current.id });
    } catch (error) { if (session === current) { wave(false); show(String(error), true); session = null; } }
  }
  input.title = "按住左键说话，松开结束；Esc 也可结束";
  async function start(source, pointerId) {
    if(!enabled())return;
    if (session) return;
    if (!ready || composing || input.disabled || input.readOnly || (source === "mouse" && input.selectionStart !== input.selectionEnd)) return;
    const id = Date.now();
    session = { id, source, draft: createVoiceDraft(input.value, input.selectionStart, input.selectionEnd), discard: false, stopping: false };
    // Capture only after the hold threshold so ordinary clicking/selection stays natural.
    if (source === "mouse") {
      try { input.setPointerCapture(pointerId); } catch { /* Pointer may already have been released. */ }
    }
    input.focus({ preventScroll: true });
    show("正在准备语音识别…");
    try { session.starting = invoke("start_voice_input", { id, source }); await session.starting; }
    catch (error) { if (session?.id === id) { session = null; wave(false); show(String(error), true); } }
  }
  const press = createLongPress({
    release: () => { if (session?.source === "mouse") stop(); },
    trigger: pointerId => start("mouse", pointerId),
  });
  listen("gesture-voice", ({ payload }) => {
    if (payload.active === true) start("gesture");
    else if (session?.source === "gesture") stop();
  }).catch(error => show(String(error), true));
  listen("voice-input", ({ payload }) => {
    if (!session || payload.id !== session.id) return;
    if (payload.state === "loading") {
      if (!session.stopping) show(payload.text);
    } else if (payload.state === "listening") {
      if (!session.stopping) { wave(true); show(session.source === "gesture" ? "正在听 · 松拳结束" : "正在听 · 松开结束"); }
    } else if (payload.state === "result" && !session.discard) {
      const update = session.draft.append(payload.text, input.value, input.selectionStart);
      if (!update) { stop(true); return; }
      input.value = update.value;
      input.setSelectionRange(update.caret, update.caret);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    } else if (payload.state === "stopping") {
      wave(false);
      show("正在结束语音输入…");
    } else if (payload.state === "stopped" || payload.state === "error") {
      wave(false);
      session = null;
      show(payload.state === "error" ? payload.text : "语音输入已结束", true);
    }
  }).then(() => { ready = true; }).catch(error => show(String(error), true));
  input.addEventListener("pointerdown", e => { if (!composing) press.down(e); });
  document.addEventListener("pointermove", press.move);
  for (const event of ["pointerup", "pointercancel"]) document.addEventListener(event, press.up);
  input.addEventListener("lostpointercapture", press.stop);
  document.addEventListener("keydown", e => {
    press.stop();
    if (session && e.key === "Escape") { e.preventDefault(); e.stopImmediatePropagation(); stop(); }
    else if (session && e.key === "Enter" && !e.shiftKey && !e.isComposing) { stop(true); }
  }, true);
  input.addEventListener("beforeinput", () => { if (session) stop(true); });
  input.addEventListener("pointerleave", press.leave);
  window.addEventListener("blur", () => { press.stop(); stop(true); });
  input.addEventListener("compositionstart", () => { composing = true; press.stop(); stop(true); });
  input.addEventListener("compositionend", () => { composing = false; });
}

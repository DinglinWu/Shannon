import test from "node:test";
import assert from "node:assert/strict";
import { createLongPress, createVoiceDraft, initVoiceInput } from "../src/voice-input.js";

function fixture() {
  let callback, delay, calls = 0, releases = 0;
  const press = createLongPress({ trigger: () => calls++, release: () => releases++, schedule: (fn, ms) => { callback = fn; delay = ms; return 1; }, cancel: () => { callback = undefined; } });
  const event = { button: 0, clientX: 10, clientY: 10, pointerId: 1, buttons: 1 };
  return { press, event, fire: () => { const fn = callback; callback = undefined; fn?.(); }, calls: () => calls, releases: () => releases, delay: () => delay };
}
test("left press triggers once after 650ms", () => {
  const f = fixture(); f.press.down(f.event); assert.equal(f.delay(), 650);
  f.fire(); f.fire(); assert.equal(f.calls(), 1);
});
test("short click, cancelled pointer and dragging do not invoke voice", () => {
  for (const action of [f => f.press.stop(), f => f.press.move({...f.event, clientX: 19}), f => f.press.move({...f.event, buttons: 0})]) {
    const f = fixture(); f.press.down(f.event); action(f); f.fire(); assert.equal(f.calls(), 0);
  }
});
test("right click, modifiers and secondary pointers do not invoke voice", () => {
  for (const change of [{button: 2}, {ctrlKey: true}, {altKey: true}, {shiftKey: true}, {metaKey: true}, {isPrimary: false}]) {
    const f = fixture(); f.press.down({...f.event, ...change}); f.fire(); assert.equal(f.calls(), 0);
  }
});
test("small pointer jitter preserves the long press", () => {
  const f = fixture(); f.press.down(f.event); f.press.move({...f.event, clientX: 13, clientY: 12}); f.fire(); assert.equal(f.calls(), 1);
});

test("release ends an activated hold exactly once, including leaving the input", () => {
  const f = fixture(); f.press.down(f.event); f.fire(); f.press.leave();
  assert.equal(f.releases(), 0);
  f.press.up({...f.event, type: "pointerup", buttons: 0}); f.press.stop();
  assert.equal(f.releases(), 1);
});

test("unrelated pointers and right-button releases do not end a held recording", () => {
  const f = fixture(); f.press.down(f.event); f.fire();
  f.press.move({...f.event, pointerId: 2, buttons: 0});
  f.press.down({...f.event, button: 2});
  f.press.up({...f.event, button: 2});
  f.press.up({...f.event, pointerId: 2});
  assert.equal(f.releases(), 0);
  f.press.up({...f.event, type: "pointercancel"}); assert.equal(f.releases(), 1);
});

test("release before activation never starts recognition, lost left button stops it", () => {
  const f = fixture(); f.press.down(f.event); f.press.up(f.event); f.fire();
  assert.equal(f.calls(), 0); assert.equal(f.releases(), 0);
  f.press.down(f.event); f.fire(); f.press.move({...f.event, buttons: 0});
  assert.equal(f.releases(), 1);
});
test("speech inserts at the caret and preserves the suffix", () => {
  const draft = createVoiceDraft("今天，明天再做", 2);
  const first = draft.append("买牛奶", "今天，明天再做", 2);
  assert.deepEqual(first, {value: "今天买牛奶，明天再做", caret: 5});
  assert.deepEqual(draft.append("和面包", first.value, first.caret), {value: "今天买牛奶和面包，明天再做", caret: 8});
});
test("typing or changing caret cancels late speech insertion", () => {
  assert.equal(createVoiceDraft("原文", 2).append("后续", "修改后的内容", 2), null);
  assert.equal(createVoiceDraft("原文", 2).append("后续", "原文", 0), null);
});
test("English speech chunks remain separated", () => {
  assert.deepEqual(createVoiceDraft("buy", 3).append("milk", "buy", 3), {value: "buy milk", caret: 8});
});

test("release during startup stops the same session and still inserts its final phrase", async t => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  class Element extends EventTarget {
    children = [];
    classes = new Set();
    classList = { toggle: (name, on) => on ? this.classes.add(name) : this.classes.delete(name) };
    append(...elements) { this.children.push(...elements); }
    setAttribute() {}
    focus() {}
    setPointerCapture() {}
    setSelectionRange(start, end) { this.selectionStart = start; this.selectionEnd = end; }
  }
  const doc = new Element(), win = new Element(), host = new Element(), input = new Element();
  doc.createElement = () => new Element();
  input.parentNode = host; input.value = ""; input.selectionStart = input.selectionEnd = 0;
  const previous = { document: globalThis.document, window: globalThis.window };
  globalThis.document = doc; globalThis.window = win;
  t.after(() => { for (const [key, value] of Object.entries(previous)) { if (value === undefined) delete globalThis[key]; else globalThis[key] = value; } });
  let notify, gesture, started;
  const start = new Promise(resolve => { started = resolve; });
  const calls = [];
  initVoiceInput(input, (name, args) => { calls.push({ name, args }); return name === "start_voice_input" ? start : Promise.resolve(); }, async (name, fn) => { if (name === "voice-input") notify = fn; else if (name === "gesture-voice") gesture = fn; });
  await Promise.resolve();
  function pointer(target, type, buttons) {
    const event = new Event(type);
    Object.assign(event, { button: 0, pointerId: 1, isPrimary: true, clientX: 10, clientY: 10, buttons });
    target.dispatchEvent(event);
  }
  pointer(input, "pointerdown", 1); t.mock.timers.tick(650);
  const id = calls[0].args.id;
  pointer(doc, "pointerup", 0);
  assert.equal(calls.length, 1, "stop waits for the start command to be accepted");
  notify({ payload: { id, state: "listening" } });
  assert.equal(host.classes.has("voice-listening"), false, "late listening must not restart released waves");
  started(); await Promise.resolve(); await Promise.resolve();
  assert.deepEqual(calls[1], { name: "stop_voice_input", args: { id } });
  notify({ payload: { id, state: "result", text: "最后一句。" } });
  assert.equal(input.value, "最后一句。");
  notify({ payload: { id, state: "stopped" } });
  assert.equal(host.classes.has("voice-listening"), false);
  // Showing the launcher selects existing text: gesture dictation must still start.
  input.setSelectionRange(0, input.value.length);
  gesture({ payload: { active: true } });
  assert.equal(calls[2].args.source, "gesture");
  const gestureId = calls[2].args.id;
  await Promise.resolve();
  pointer(doc, "pointerup", 0);
  assert.equal(calls.length, 3, "mouse release cannot end gesture dictation");
  notify({ payload: { id: gestureId, state: "result", text: "新的一句。" } });
  assert.equal(input.value, "新的一句。");
  gesture({ payload: { active: false } });
  await Promise.resolve(); await Promise.resolve();
  assert.deepEqual(calls[3], { name: "stop_voice_input", args: { id: gestureId } });
  notify({ payload: { id: gestureId, state: "stopped" } });
});

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const source = readFileSync(new URL("../src/notes.js", import.meta.url), "utf8");
function setup(invoke) {
  const state = { notes: [{ id: 1, done: false }], busy: new Set(), error: { textContent: "" }, count() {}, invoke };
  const button = { value: "false", setAttribute(_, value) { this.value = value; } };
  vm.createContext(state);
  vm.runInContext(source.slice(source.indexOf("async function toggle("), source.indexOf("const tab =")), state);
  return { state, button };
}

test("crossing off and restoring saves both states", async () => {
  const calls = [];
  const { state, button } = setup(async (_, args) => calls.push(args.done));
  await state.toggle(1, button);
  assert.equal(button.value, "true");
  assert.equal(state.notes[0].done, true);
  await state.toggle(1, button);
  assert.equal(button.value, "false");
  assert.deepEqual(calls, [true, false]);
});

test("failed save restores the displayed state and reports failure", async () => {
  const { state, button } = setup(async () => { throw new Error("disk full"); });
  await state.toggle(1, button);
  assert.equal(button.value, "false");
  assert.equal(state.notes[0].done, false);
  assert.match(state.error.textContent, /disk full/);
  assert.equal(state.busy.size, 0);
});

test("rapid clicks cannot overlap writes for the same note", async () => {
  let resolve;
  let calls = 0;
  const { state, button } = setup(() => { calls++; return new Promise(r => { resolve = r; }); });
  const first = state.toggle(1, button);
  await state.toggle(1, button);
  assert.equal(calls, 1);
  resolve();
  await first;
  assert.equal(state.notes[0].done, true);
  assert.equal(state.busy.size, 0);
});

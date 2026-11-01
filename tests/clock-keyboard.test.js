import test from "node:test";
import assert from "node:assert/strict";
import { modeForArrow } from "../src/clock-keyboard.js";

test("right switches to count-up only at the end of an unselected input", () => {
  assert.equal(modeForArrow("down", "ArrowRight", { start: 1, end: 1, length: 2 }), null);
  assert.equal(modeForArrow("down", "ArrowRight", { start: 0, end: 2, length: 2 }), null);
  assert.equal(modeForArrow("down", "ArrowRight", { start: 2, end: 2, length: 2 }), "up");
});
test("left switches from count-up back to countdown", () => {
  assert.equal(modeForArrow("up", "ArrowLeft"), "down");
  assert.equal(modeForArrow("up", "ArrowRight"), null);
  assert.equal(modeForArrow("down", "ArrowLeft"), null);
});
test("left preserves input editing before the start boundary", () => {
  assert.equal(modeForArrow("up", "ArrowLeft", { start: 1, end: 1, length: 2 }), null);
  assert.equal(modeForArrow("up", "ArrowLeft", { start: 0, end: 0, length: 2 }), "down");
});
test("other keys cannot switch modes, and an empty input has a right boundary", () => {
  assert.equal(modeForArrow("down", "ArrowUp"), null);
  assert.equal(modeForArrow("down", "ArrowRight", { start: 0, end: 0, length: 0 }), "up");
});

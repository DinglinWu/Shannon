import test from "node:test";
import assert from "node:assert/strict";
import { Clock, parseDuration, formatTime } from "../src/clock-model.js";

test("duration supports minutes, units and clock notation", () => {
  for (const [input, ms] of [["5", 300000], ["1.5", 90000], ["5m", 300000], ["90s", 90000], ["1h 2m 3s", 3723000], ["01:30", 90000], ["1:02:03", 3723000], ["24h", 86400000]]) {
    assert.equal(parseDuration(input), ms, input);
  }
});
test("rejects invalid, zero, negative and excessive durations", () => {
  for (const input of ["", "0", "0s", "-5", "25h", "1:60", "1:02:60", "abc", "Infinity", "0.0000001", "1m junk"]) {
    assert.equal(parseDuration(input), null, input);
  }
});
test("pause and resume exclude paused time and do not reset elapsed time", () => {
  const clock = new Clock("up");
  clock.resume(1000);
  clock.resume(2000);
  clock.pause(3500);
  assert.equal(clock.elapsedAt(10000), 2500);
  clock.resume(12000);
  assert.equal(clock.elapsedAt(15000), 5500);
});
test("countdown catches up after delayed callbacks and never goes negative", () => {
  const clock = new Clock("down", 5000);
  clock.resume(1000);
  assert.equal(clock.remaining(2500), 3500);
  assert.equal(clock.remaining(90000), 0);
});
test("formatting rounds countdown up and elapsed down, including hour rollover", () => {
  assert.equal(formatTime(1, true), "00:01");
  assert.equal(formatTime(1999), "00:01");
  assert.equal(formatTime(0, true), "00:00");
  assert.equal(formatTime(3600000), "01:00:00");
  assert.equal(formatTime(-1), "00:00");
});

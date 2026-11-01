import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
import { themeActivationKey } from "../src/theme-activation.js";

const holiday = { id: "national-day", when: { from: "10-01", to: "10-07" }, enter: { message: "国庆快乐" } };
const source = fs.readFileSync(new URL("../src/theme.js", import.meta.url), "utf8");
const activationCode = source.slice(source.indexOf("async function activateTheme("), source.indexOf("// ---------- 唤起特效 ----------"));
const settle = () => new Promise(setImmediate);

function harness(saved = null, hidden = false) {
  const events = [], timers = [], classes = new Set(hidden ? ["window-hidden"] : []);
  const context = {
    themeActivationKey, today: () => "2026-10-01", current: null,
    activationSeq: 0, applySeq: 0, appliedActivationKey: null, pendingActivation: null,
    ACTIVE_THEME_KEY: "active", MESSAGE_STEP: 70,
    document: { documentElement: { classList: {
      add: (name) => classes.add(name), remove: (name) => classes.delete(name), contains: (name) => classes.has(name),
    } } },
    els: { appEl: { offsetWidth: 640 } },
    hideGreeting() {}, load: () => saved, store: (_, value) => { saved = value; },
    setTimeout: (callback) => { timers.push(callback); },
    playEnter: async (_, theme) => { events.push("intro:" + theme.id); return true; },
    apply: async (theme) => {
      events.push("apply:" + (theme?.id || "default"));
      context.current = theme;
      context.appliedActivationKey = themeActivationKey(theme, context.today());
      saved = context.appliedActivationKey;
    },
  };
  vm.createContext(context);
  vm.runInContext(activationCode, context);
  return { context, events, timers, classes };
}

test("multi-day festival has one activation key and resets next year", () => {
  assert.equal(themeActivationKey(holiday, "2026-10-01"), themeActivationKey(holiday, "2026-10-07"));
  assert.notEqual(themeActivationKey(holiday, "2026-10-01"), themeActivationKey(holiday, "2027-10-01"));
  const crossYear = { id: "new-year", when: { from: "12-31", to: "01-02" } };
  assert.equal(themeActivationKey(crossYear, "2026-12-31"), themeActivationKey(crossYear, "2027-01-01"));
});
test("first activation shows intro before applying colors; repeat preview does not reapply", async () => {
  const h = harness();
  const task = h.context.activateTheme(holiday);
  await settle();
  assert.deepEqual(h.events, ["intro:national-day"]);
  h.timers.shift()(); await settle();
  assert.deepEqual(h.events, ["intro:national-day", "apply:national-day"]);
  h.timers.shift()(); await task;
  await h.context.activateTheme(holiday, true);
  assert.deepEqual(h.events, ["intro:national-day", "apply:national-day", "intro:national-day"]);
});
test("restart restores an existing festival without an intro; unseen festival waits for reveal", async () => {
  const known = harness(themeActivationKey(holiday, "2026-10-01"), true);
  await known.context.requestTheme(holiday);
  assert.deepEqual(known.events, ["apply:national-day"]);
  const first = harness(null, true);
  await first.context.requestTheme(holiday);
  assert.deepEqual(first.events, []);
});
test("switching off during the intro prevents the delayed theme application", async () => {
  const h = harness();
  const task = h.context.activateTheme(holiday);
  await settle();
  await h.context.activateTheme(null);
  h.timers.shift()(); await task;
  assert.deepEqual(h.events, ["intro:national-day"]);
  assert.equal(h.context.appliedActivationKey, null);
});

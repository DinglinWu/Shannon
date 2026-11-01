import test from "node:test";
import assert from "node:assert/strict";
import { greetingLayout } from "../src/theme-greeting.js";

test("greeting is centered above the search box with a clear gap", () => {
  const anchor = { left: 320, top: 330, width: 640 };
  const layout = greetingLayout(anchor, { width: 1280, height: 720 });
  assert.equal(layout.left + layout.width / 2, anchor.left + anchor.width / 2);
  assert.equal(layout.top + layout.height, anchor.top - 12);
});
test("greeting stays inside screen edges on narrow screens and offset windows", () => {
  for (const left of [-50, 1800]) {
    const layout = greetingLayout({ left, top: 300, width: 640 }, { width: 900, height: 720 });
    assert.ok(layout.left >= 16);
    assert.ok(layout.left + layout.width <= 884);
  }
});
test("limited space uses compact layout and insufficient space never covers input", () => {
  const compact = greetingLayout({ left: 0, top: 60, width: 640 }, { width: 900, height: 720 });
  assert.equal(compact.height, 48);
  assert.equal(compact.top + compact.height, 48);
  assert.equal(greetingLayout({ left: 0, top: 24, width: 640 }, { width: 900, height: 720 }), null);
});

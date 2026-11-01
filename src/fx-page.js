// 全屏特效窗口的页面脚本：取参数 → 放特效 → 通知 Rust 把窗口销毁
import { invoke } from "./invoke.js";
import { runFx } from "./fx.js";
import { listen } from "@tauri-apps/api/event";
import { renderGreeting } from "./theme-greeting.js";
import "./theme-greeting.css";

let greeting = null;
function showGreeting(spec) {
  greeting?.dismiss(true);
  greeting = spec ? renderGreeting(spec) : null;
}

async function main() {
  await listen("dismiss-greeting", ({ payload }) => greeting?.dismiss(payload === true));
  await listen("show-greeting", ({ payload }) => showGreeting(payload));
  const payload = await invoke("take_fx_payload");
  if (payload) {
    showGreeting(payload.greeting);
    await runFx(document.getElementById("fx"), payload, payload.images || [], "screen");
    // 手动预览另一个主题时，以最新寄语的淡出时间为准。
    while (greeting) {
      const active = greeting;
      await active.finished;
      if (active === greeting) break;
    }
  }
  await invoke("fx_done");
}

main().catch(() => invoke("fx_done"));

export const gestureHelp = `手势快捷操作（默认）与可选鼠标模式：

- \`/gesture start\` 按当前模式开启手势控制（后台运行）
- \`/gesture install\` 按需安装手势组件；握拳语音还需要 \`/voice install\`
- \`/gesture stop\` 关闭识别并释放摄像头
- \`/gesture status\` 查看运行、追踪和摄像头状态
- \`/gesture preview\` 打开摄像头预览，默认等待并拢手掌朝向摄像头唤醒
- \`/gesture cameras\` 列出摄像头名称和编号
- \`/gesture config\` 查看配置
- \`/gesture config hand Left\` 切换控制手
- \`/gesture config camera 0\` 选择摄像头
- \`/gesture config mode shortcuts\` 使用快捷操作模式
- \`/gesture config mode mouse\` 切回鼠标模式
- \`/gesture bind pinch launcher\` 捏合唤起搜索框（Alt+Space）
- \`/gesture bind swipe_right page_next\` 右挥翻到下一页
- \`/gesture bind swipe_left page_previous\` 左挥翻到上一页
- \`/gesture bind swipe_up none\` 禁用上挥动作
- \`/gesture config margin 0.2\` 调整操作区域
- \`/gesture config {"pinch_on":0.28,"pinch_off":0.42}\` 批量配置
- \`/gesture restart\` 重启并应用配置，重新等待手掌唤醒

配置会保存到手势项目的 config.json。运行期间修改配置后，使用 restart 应用。
默认：捏合播放/暂停；四指水平挥动切歌，双指在画面内旋转调音量（偏转越大，变化越快）；回中后可切换功能，动作中不响应捏合。
唤醒后，握拳并稳定约半秒，会唤起搜索框开始本地语音输入；保持握拳说话，松拳结束。Esc 也可结束，文字不会自动发送。右摆切换下一曲无需特定手掌或手背朝向。
手形稳定约 0.35 秒后选中功能；动作回中后经过冷却才可再次触发。所有映射可在 config 中查看。
动作：play_pause / previous_track / next_track / volume_up / volume_down / mute /
page_next / page_previous / launcher / none。
也可使用中文命令前缀 \`/手势\`。`;

export function parseGestureCommand(arg = "") {
  const text = arg.trim();
  const match = text.match(/^(\S+)(?:\s+([\s\S]*))?$/);
  const action = (match?.[1] || "status").toLowerCase();
  const rest = (match?.[2] || "").trim();
  if (action === "help" || action === "?") return { help: true };
  if (action === "bind") {
    const binding = rest.match(/^(pinch|swipe_left|swipe_right|swipe_up|swipe_down)\s+(\w+)$/);
    if (!binding) throw new Error("用法：/gesture bind pinch launcher；help 查看手势与动作列表");
    return { action: "configure", payload: { bindings: { [binding[1]]: binding[2] } } };
  }
  if (action === "config") {
    if (!rest) return { action: "config", payload: {} };
    let payload;
    if (rest.startsWith("{")) {
      payload = JSON.parse(rest);
    } else {
      const pair = rest.match(/^(\w+)\s+(.+)$/);
      if (!pair) throw new Error("用法：/gesture config 参数 值，或 /gesture config JSON对象");
      let value;
      if (pair[1] === "hand") {
        const hand = pair[2].toLowerCase();
        value = hand === "left" ? "Left" : hand === "right" ? "Right" : pair[2];
      } else if (pair[1] === "mode") {
        value = pair[2].toLowerCase();
      } else {
        value = JSON.parse(pair[2]);
      }
      payload = { [pair[1]]: value };
    }
    if (!payload || Array.isArray(payload) || typeof payload !== "object") throw new Error("配置必须是 JSON 对象");
    return { action: "configure", payload };
  }
  if (rest) throw new Error("该命令不接受额外参数；输入 /gesture help 查看用法");
  if (action === "preview") return { action: "restart", payload: { preview: true, dry_run: false, active: false } };
  if (!["start", "stop", "status", "restart", "cameras"].includes(action)) {
    throw new Error("未知手势命令；输入 /gesture help 查看用法");
  }
  return { action, payload: {} };
}

export function formatGestureResult(action, data) {
  if (action === "cameras") return data.length
    ? "可用摄像头：\n\n" + data.map(c => `- **${c.index}**：${c.name}`).join("\n")
    : "没有检测到摄像头。";
  if (action === "config") return "手势配置：\n\n```json\n" + JSON.stringify(data, null, 2) + "\n```";
  if (action === "configure") return "配置已保存。" + (data.restart_required
    ? "当前运行配置尚未改变，输入 `/gesture restart` 应用。" : "下次开启时生效。")
    + "\n\n```json\n" + JSON.stringify(data.config, null, 2) + "\n```";
  const mode = data.mode || data.effective_config?.mode || data.config?.mode || 'shortcuts';
  const state = data.phase === "error" ? "启动或运行失败" : !data.process_running ? "已关闭"
    : data.telemetry_stale ? "状态更新超时" : data.phase === "starting" ? "正在启动"
    : data.recognition_active ? "手势识别窗口已开启" : "等待四指并拢、掌心朝向摄像头唤醒";
  let output = `手势控制：**${state}**\n\n模式：${mode === 'shortcuts' ? '快捷操作（不移动光标）' : '鼠标控制'}`;
  if (data.camera_name) output += `\n\n摄像头：${data.camera} · ${data.camera_name}`;
  if (data.tracking) output += `\n\n追踪状态：${data.tracking}；${data.fps || 0} FPS`;
  if (data.detected) output += `\n\n检测结果：${data.detected}`;
  if (data.last_action) output += `\n\n最近动作：${data.last_action.gesture} → ${data.last_action.action}`;
  if (data.dry_run) output += "\n\n当前为测试模式，不输出鼠标或键盘事件。";
  if (data.error) output += "\n\n错误：\n```text\n" + data.error + "\n```";
  return output;
}

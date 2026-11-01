export function parseComponentCommand(prompt) {
  const match = prompt.trim().match(/^[\/／](?:components?|组件)(?:\s+(.*))?$/i);
  const shortcut = prompt.trim().match(/^[\/／](voice|语音|gesture|手势)\s+(install|安装)$/i);
  if (shortcut) return { action: 'install', name: /^(voice|语音)$/i.test(shortcut[1]) ? 'voice' : 'gesture' };
  if (!match) return null;
  const parts = (match[1] || 'status').trim().split(/\s+/);
  if (['help', '?'].includes(parts[0]) && parts.length === 1) return { action: 'help' };
  if (parts[0] === 'status' && parts.length === 1) return { action: 'status' };
  if (parts[0] === 'install' && parts.length === 2 && ['voice', 'gesture'].includes(parts[1])) return { action: 'install', name: parts[1] };
  throw new Error('用法：/component status，或 /component install voice|gesture');
}
export const componentHelp = `可选组件默认不随应用安装，下载后才能使用：

- \`/component status\` 查看安装状态和进度
- \`/component install voice\` 安装离线语音识别（也可输入 \`/voice install\`）
- \`/component install gesture\` 安装手势识别（也可输入 \`/gesture install\`）

只下载所选组件，安装后不会自动开启麦克风或摄像头。语音装好即可长按说话；手势装好后输入 \`/gesture start\`。握拳语音需要两个组件都安装。`;
export function componentProgress(data) {
  const name = data.name === 'voice' ? '本地语音' : '手势识别';
  if (data.phase === 'installed') return `${name}组件已安装。${data.name === 'voice' ? '在输入框内按住左键即可说话。' : '输入 /gesture start 开启。'}`;
  if (data.phase === 'error') return `${name}组件安装失败：${String(data.error || '未知错误')}\n\n可输入 /component install ${data.name} 重试。`;
  const percent = data.total ? Math.min(100, Math.floor(data.received / data.total * 100)) : 0;
  return `${name}组件：${data.phase === 'verifying' ? '正在校验并安装…' : `正在下载 ${percent}%…`}\n\n安装后不会自动启用设备。`;
}
export function formatComponents(data) {
  return '可选组件：\n\n' + data.components.map(item => {
    const name = item.name === 'voice' ? '本地语音' : '手势识别';
    if (item.job && !['installed', 'error'].includes(item.job.phase)) return componentProgress(item.job);
    return `- **${name}**：${item.installed ? '已安装' : '未安装'}${item.download_bytes ? `，下载约 ${Math.round(item.download_bytes / 1024 / 1024)} MB` : ''}${item.job?.phase === 'error' ? '（上次安装失败）' : ''}`;
  }).join('\n\n') + '\n\n输入 `/component help` 查看安装命令。';
}

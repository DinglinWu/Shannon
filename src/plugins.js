export function parsePluginCommand(prompt) {
  const direct = prompt.trim().match(/^[\/／]([a-z][a-z0-9-]{0,23})\/([a-z][a-z0-9_]{0,23})(?:\s+([\s\S]+))?$/);
  if (direct) return parsePluginCommand(`/plugin run ${direct[1]}/${direct[2]} ${direct[3] || '{}'}`);
  const match = prompt.trim().match(/^[\/／](?:plugins?|插件)(?:\s+([\s\S]*))?$/i);
  if (!match) return null;
  const text = (match[1] || 'manage').trim();
  if(['manage','管理'].includes(text))return {action:'manage'};
  if (['help','?','帮助'].includes(text)) return {action:'help'};
  if (['list','列表'].includes(text)) return {action:'list'};
  const install = text.match(/^(?:install|安装)\s+([\s\S]+)$/);
  if (install) {
    let source = install[1].trim();
    if (/^["']/.test(source)) {
      if (source.at(-1) !== source[0]) throw new Error('插件路径引号未闭合');
      source = source.slice(1,-1);
    }
    if (!source || source.length > 2048) throw new Error('插件来源无效');
    return {action:'install',source};
  }
  const change = text.match(/^(info|enable|disable|uninstall)\s+([a-z][a-z0-9-]{0,23})$/);
  if (change) return {action:change[1],id:change[2]};
  const run = text.match(/^run\s+([a-z][a-z0-9-]{0,23})\/([a-z][a-z0-9_]{0,23})(?:\s+([\s\S]+))?$/);
  if (run) {
    let args;
    try { args = JSON.parse(run[3] || '{}'); } catch { throw new Error('插件参数须为有效 JSON 对象'); }
    if (!args || Array.isArray(args) || typeof args !== 'object') throw new Error('插件参数须为 JSON 对象');
    if (JSON.stringify(args).length > 32768) throw new Error('插件参数过大');
    return {action:'run',id:run[1],tool:run[2],args};
  }
  throw new Error('用法：/plugin list、install 插件ZIP路径或HTTPS地址、info 插件ID、enable/disable/uninstall 插件ID、run 插件ID/工具名 {JSON参数}');
}

// Fence untrusted metadata/results instead of inserting plugin Markdown into
// the host DOM. A fence longer than every payload run cannot be closed by it.
export function pluginValue(value) {
  const text = JSON.stringify(value, null, 2) ?? 'null';
  const fence = '~'.repeat(Math.max(3, ...(text.match(/~+/g) || []).map(s => s.length + 1)));
  return `${fence}json\n${text}\n${fence}`;
}
const safeText = text => String(text).replace(/\s+/g, ' ').replace(/[\\`*_{}[\]()#+.!<>|$~:/-]/g, '\\$&');
export function formatPluginList(data) {
  if (!data.plugins?.length) return '尚未安装插件。\n\n输入 `/plugin install 插件ZIP的完整路径` 安装，或输入 `/plugin help` 查看用法。';
  return '**已安装的插件**\n\n' + data.plugins.map(item => {
    const m = item.manifest;
    return `- **${safeText(m.name)}** · ${item.enabled ? '已启用' : '已停用'} · v${safeText(m.version)}\n  ${safeText(m.description)}\n  查看工具：\`/plugin info ${m.id}\``;
  }).join('\n\n') + '\n\n输入 `/plugin help` 查看管理命令。';
}
export function formatPluginInfo(item) {
  const m = item.manifest;
  return `**${safeText(m.name)}** · v${safeText(m.version)} · ${item.enabled ? '已启用' : '已停用'}\n\n${safeText(m.description)}\n\n**工具**\n\n`
    + m.tools.map(t => `\`/${m.id}/${t.name}\` — ${safeText(t.description)}\n\n参数：\n\n${pluginValue(t.parameters)}`).join('\n\n')
    + '\n\n**权限**\n\n' + (m.permissions.length ? m.permissions.map(p => `- ${safeText(p)}`).join('\n') : '不申请宿主权限');
}
export function formatBuiltinResult(id,tool,data) {
  if(id==='broadcast'){if(tool==='open')return '已打开广播站，开启共享后同一网络的人可通过浏览器查看。';if(['prepare','withdraw'].includes(tool))return '已打开广播确认预览，点击确认后才会生效。';}
  if (id==='wechat' && tool==='open') return `已打开与 ${safeText(data.peer_name)} 的 AI 模拟聊天。`;
  if (id==='clock' && tool==='open') return data.already_active
    ? '已有计时器正在运行。如需替换，可输入 `@clock 替换当前计时器并重新设置`。'
    : '已打开计时设置，确认后开始。';
  if (id==='clock' && tool==='status') return data.active ? '计时器正在运行。' : '当前没有正在运行的计时器。';
  if (id==='note') {
    const message={show:'已展开便签。',close:'已退出便签，内容已保留。',add:'已添加便签。',set_date:'已更新待办日期。',delete:'已移入最近删除，可在便签中恢复。',delete_many:`已将 ${data.count} 条便签移入最近删除，可恢复。`,restore_deleted:'已恢复删除的便签。',reorder:'已更新待办顺序。',set_done:data.done?'已划去便签。':'已恢复便签。'}[tool];
    if (message) return message + (data.display_error ? '\n\n内容已保存，但便签图标未能显示：\n\n'+pluginValue(data.display_error) : '');
  }
  return pluginValue(data);
}
export const pluginHelp = `管理内置与第三方插件：

- 输入 \`/plugin\` 打开统一插件管理，可添加、启停和移除内置功能，或安装第三方ZIP


- 输入 \`@broadcast\` 打开局域网广播站；准备广播后在窗口确认发布，同一网络通过访问码查看
- 输入 \`@\` 选择插件或工具，再用自然语言描述需求，例如 \`@quick-stats 计算 2、4、6 的平均值\`
- ↑↓ 选择，Tab / Enter 插入；\`@插件ID/工具名 {"参数":"值"}\` 可不经过 AI 直接执行
- \`/plugin install E:\\插件\\示例.zip\` 从本机安装，也支持 HTTPS ZIP 下载地址
- \`/plugin list\` 查看已装插件
- \`/plugin info 插件ID\` 查看工具、参数和权限
- \`/plugin run 插件ID/工具名 {"参数":"值"}\` 执行工具
- 也可直接输入 \`/插件ID/工具名 {"参数":"值"}\`，输入 / 时可在提示中选择
- \`/plugin enable 插件ID\` 启用
- \`/plugin disable 插件ID\` 停用
- \`/plugin uninstall 插件ID\` 卸载并保留插件数据

安装、升级及启停/卸载均会打开确认窗口。启用后，插件工具自动加入 AI 工具目录。第三方描述不代表闪念认可；请在安装前核对来源与权限。`;

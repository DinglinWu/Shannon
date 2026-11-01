import { invoke } from './invoke.js';
import './plugins-confirm.css';
const $ = selector => document.querySelector(selector);
const permissions = {
  'storage.read':'读取本插件的独立存储', 'storage.write':'修改本插件的独立存储',
  'notes.organize':'调整便签日期与顺序', 'notes.delete':'删除或恢复最近删除的便签', 'notes.trash.read':'读取最近删除的便签',
  'notes.read':'读取你的便签内容', 'notes.write':'添加便签、划去或恢复便签',
  'notes.display':'展开或退出便签窗口', 'clock.read':'查询计时器状态', 'clock.display':'打开计时设置（仍需你确认开始）',
};
let request;
function list(selector, values) { for (const text of values) { const li = document.createElement('li');li.textContent = text;$(selector).append(li); } }
async function load() { try {
  request = await invoke('plugin_review');
  const m = request.manifest;
  const action = request.builtin?{install:'添加',enable:'启用',disable:'停用',uninstall:'移除'}[request.action]:{install:'安装 / 更新',enable:'启用',disable:'停用',uninstall:'卸载'}[request.action];
  $('#title').textContent = `${action}插件`;
  $('#identity').textContent = `${m.name} · v${m.version} · ${m.author}`;
  $('#description').textContent = m.description;
  $('#readme').textContent = request.readme || '此旧版安装没有保存 README；重新安装新版 ZIP 后可查看。';
  $('#source').textContent = request.source;
  if(request.builtin){document.querySelector('section .hint').textContent=request.official_source?'本地官方包已核对当前版本的大小与 SHA-256，确认后再次校验并导入；不会联网下载。':'官方插件来自项目 Gitee 发行版，下载后核对大小与 SHA-256；确认前不下载或启用。';}
  $('#hash').textContent = request.sha256;
  list('#permissions', request.builtin?['官方插件；语音和手势的模型/环境仍需单独安装，设备需手动启动']:m.permissions.length ? m.permissions.map(p => permissions[p] || p) : ['不申请宿主权限，仅处理传入的参数']);
  list('#tools', request.builtin?['入口、命令提示与 AI 调用能力一起管理，保留现有业务数据']:m.tools.map(t => `${t.name}：${t.description}（${{read:'只读',display:'展示',write:'修改'}[t.effect]}）`));
  $('#consequence').textContent = request.builtin&&request.official_source?'已读取并核对 ZIP 内的 README 和插件信息，确认后导入对应插件包；业务数据保留。':request.builtin?'官方目录提供预期 README，下载后再次校验包内原文。各插件分别安装、独立运行。移除或停用保留业务数据；相关窗口或设备仍在使用时需先关闭或停止。':request.action === 'uninstall'
    ? '确认后移除命令和 AI 工具注册，保留插件数据和安装缓存，便于重新安装后恢复。'
    : request.action === 'disable' ? '确认后立即停用插件，后续命令和 AI 将无法调用其工具。'
    : '确认后启用这些工具，供你通过搜索命令或 AI 调用。每次调用在隔离线程中运行；插件不能直接访问系统文件、网络、麦克风或摄像头。';
  $('#approve').textContent = `确认${action}`;
  $('#approve').disabled = false;
} catch(error) { $('#error').textContent = String(error); } }
load();
async function confirm(accepted) {
  if (!request) return;
  $('#approve').disabled = true;$('#cancel').disabled = true;
  try { await invoke('plugin_confirm', {token:request.token,accepted}); }
  catch(error) { $('#error').textContent=String(error);$('#approve').disabled=false;$('#cancel').disabled=false; }
}
$('#approve').onclick = () => confirm(true);
$('#cancel').onclick = () => confirm(false);
document.addEventListener('keydown', event => { if (event.key === 'Escape') confirm(false); });

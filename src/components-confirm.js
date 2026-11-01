import { invoke } from './invoke.js';
import './components-confirm.css';
let request;
const install = document.querySelector('#install');
const error = document.querySelector('#error');
async function load() { try {
  request = await invoke('component_confirmation');
  document.querySelector('#description').textContent = `需要下载${request.component === 'voice' ? '本地语音识别' : '手势识别'}组件，约 ${(request.bytes / 1024 / 1024).toFixed(0)} MB。组件包含模型和运行库，将保存在本机。`;
  install.disabled = false;
} catch (e) { error.textContent = String(e); } }
load();
async function confirm(accepted) {
  if (!request) return;
  install.disabled = true;
  try { await invoke('component_confirm', { token: request.token, accepted }); }
  catch (e) { error.textContent = String(e); install.disabled = false; }
}
install.addEventListener('click', () => confirm(true));
document.querySelector('#cancel').addEventListener('click', () => confirm(false));
document.addEventListener('keydown', e => { if (e.key === 'Escape') confirm(false); });

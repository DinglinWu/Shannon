import './downloads.css';
import {invoke} from './invoke.js';
import {listen} from '@tauri-apps/api/event';
import {taskView} from './downloads-model.js';
const $=id=>document.getElementById(id);
let snapshot=null,filter='all',loading=false,acting=false;
const cards=new Map();
function message(text=''){$('feedback').textContent=text;$('feedback').hidden=!text;}
function card(task){
 let el=cards.get(task.id);
 if(!el){
  el=document.createElement('article');el.className='task';
  el.innerHTML='<div class="task-head"><span class="task-name"></span><span class="task-status"></span></div><div class="track"><div class="bar"></div></div><div class="task-meta"></div><div class="actions"><button class="secondary" data-action="pause">暂停</button><button data-action="continue">继续</button><button class="secondary" data-action="cancel">取消任务</button></div>';
  for(const b of el.querySelectorAll('button'))b.addEventListener('click',()=>act(()=>invoke('downloads_control',{id:task.id,action:b.dataset.action})));
  cards.set(task.id,el);$('tasks').append(el);
 }
 const v=taskView(task);el.querySelector('.task-name').textContent=task.name||task.id;
 el.querySelector('.task-status').textContent=v.status;
 el.querySelector('.bar').style.width=v.percent+'%';
 el.querySelector('.task-meta').textContent=v.detail;
 el.querySelector('[data-action="pause"]').hidden=!v.canPause;
 el.querySelector('[data-action="continue"]').hidden=!v.canContinue;
 el.querySelector('[data-action="cancel"]').textContent=v.done?'移除记录':'取消任务';
 el.hidden=filter==='done'?!v.done:filter==='active'?v.done:false;
 for(const b of el.querySelectorAll('button'))b.disabled=acting;
 return !el.hidden;
}
function render(s){
 $('installer').hidden=s.installed;
 $('install').disabled=s.installing||acting;$('install').textContent=s.installing?'正在安装…':'同意安装并下载组件';
 const p=s.pending;$('review').hidden=!p;
 if(p){$('review-url').textContent=p.draft.url;$('review-folder').textContent='保存至：'+p.folder;$('review-mode').textContent=(p.draft.connections===1?'单连接':'最多4个连接')+' · 不自动运行或解压文件';}
 $('approve').disabled=!s.installed||acting;$('reject').disabled=acting;
 $('folder').textContent='下载位置：'+s.folder;
 $('count').textContent=(s.tasks||[]).length;
 let count=0;const ids=new Set();for(const t of s.tasks||[]){ids.add(t.id);if(card(t))count++;}
 for(const [id,el]of cards){if(!ids.has(id)){el.remove();cards.delete(id);}}
 $('empty').hidden=count>0;
 if(s.install_error)message(s.install_error);
}
async function refresh(){if(loading)return;loading=true;try{snapshot=await invoke('downloads_snapshot');render(snapshot);}catch(e){message(String(e));}finally{loading=false;}}
async function act(fn){if(acting)return;acting=true;message();if(snapshot)render(snapshot);try{const r=await fn();if(r?.existing)message('相同链接已有任务，请在列表中继续或查看。');}catch(e){message(String(e));}finally{acting=false;await refresh();}}
$('add-form').addEventListener('submit',e=>{e.preventDefault();act(async()=>{await invoke('downloads_prepare',{draft:{url:$('url').value.trim(),connections:Number($('connections').value)}});$('url').value='';});});
$('install').addEventListener('click',()=>act(()=>invoke('downloads_install')));
for(const [id,approve]of [['approve',true],['reject',false]])$(id).addEventListener('click',()=>{const token=snapshot?.pending?.token;if(token)act(()=>invoke('downloads_confirm',{token,approve}));});
for(const b of document.querySelectorAll('[data-filter]'))b.addEventListener('click',()=>{filter=b.dataset.filter;for(const x of document.querySelectorAll('[data-filter]'))x.classList.toggle('active',x===b);if(snapshot)render(snapshot);});
$('refresh').addEventListener('click',refresh);
let stopped=false,timer;async function poll(){await refresh();if(!stopped)timer=setTimeout(poll,1200);}
let unlisten;listen('downloads-updated',refresh).then(fn=>{if(stopped)fn();else unlisten=fn;}).catch(e=>message(String(e)));window.addEventListener('beforeunload',()=>{stopped=true;clearTimeout(timer);unlisten?.();});poll();

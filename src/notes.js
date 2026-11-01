import { invoke } from './invoke.js';
import { getCurrentWindow,availableMonitors,cursorPosition } from '@tauri-apps/api/window';
import { createAutoCollapse } from './note-auto-collapse.js';
import { createNoteWidget } from './note-widget.js';
import { dateCaption,localDate,agendaNotes,mergeVisibleOrder,quickDates } from './note-widget-model.js';
const win=getCurrentWindow(),paper=document.querySelector('#paper'),list=document.querySelector('#list'),error=document.querySelector('#error');
const reduced=matchMedia('(prefers-reduced-motion: reduce)');
let notes=[],deleted=[],revision=0,drag=null,pendingRefresh=false,dateId=null,lastDeleted=null;
let agendaView='today';
const visibleNotes=()=>agendaNotes(notes,agendaView);
const busy=new Set();
const paths={grip:'M8 5h.01M8 12h.01M8 19h.01M15 5h.01M15 12h.01M15 19h.01',date:'M5 5h14v15H5zM8 3v4M16 3v4M5 10h14',remove:'M5 7h14M9 7V4h6v3M7 7l1 13h8l1-13M10 11v5M14 11v5'};
function iconButton(kind,label){const button=document.createElement('button');button.className=kind==='grip'?'drag-handle':'icon-button';button.type='button';button.title=label;button.setAttribute('aria-label',label);const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');svg.setAttribute('viewBox','0 0 24 24');svg.setAttribute('aria-hidden','true');const path=document.createElementNS(svg.namespaceURI,'path');path.setAttribute('d',paths[kind]);svg.append(path);button.append(svg);return button;}
function count(){const remaining=visibleNotes().filter(n=>!n.done).length;const todayRemaining=agendaNotes(notes).filter(n=>!n.done).length;document.querySelector('#count').textContent=`${remaining} 件待办`;const empty=document.querySelector('#empty');empty.hidden=visibleNotes().length>0;empty.textContent=agendaView==='today'?'今天的事情，都安排好了。':agendaView==='upcoming'?'还没有安排以后的待办。':'把想做的事，先记在这里。';const badge=document.querySelector('#badge');badge.hidden=todayRemaining===0;badge.textContent=todayRemaining>99?'99+':String(todayRemaining);document.querySelector('#trash-toggle').textContent=deleted.length?`最近删除 (${deleted.length})`:'最近删除';}
function positions(){return new Map([...list.children].map(row=>[row.dataset.id,row.getBoundingClientRect().top]));}
function animateOrder(before){if(reduced.matches)return;for(const row of list.children){if(row.classList.contains('dragging'))continue;const old=before.get(row.dataset.id);if(old===undefined)continue;for(const a of row.getAnimations())a.cancel();const dy=old-row.getBoundingClientRect().top;if(Math.abs(dy)>.5)row.animate([{transform:`translateY(${dy}px)`},{transform:'none'}],{duration:230,easing:'cubic-bezier(.22,1,.36,1)'});}}
function render(){
  const before=positions(),ids=new Set(visibleNotes().map(n=>String(n.id)));
  for(const row of [...list.children])if(!ids.has(row.dataset.id))row.remove();
  for(const note of visibleNotes()){
    let row=list.querySelector(`[data-id="${note.id}"]`);
    if(!row){row=document.createElement('div');row.className='note-row';row.dataset.id=note.id;row.setAttribute('role','listitem');
      const handle=iconButton('grip','拖动调整顺序；方向键上下移动');const content=document.createElement('div');content.className='row-content';const toggleButton=document.createElement('button');toggleButton.className='note-toggle';const ink=document.createElement('span');ink.className='ink';const date=document.createElement('button');date.type='button';date.className='date-caption';date.onclick=()=>editDate(note.id);toggleButton.append(ink);content.append(toggleButton,date);
      const actions=document.createElement('div');actions.className='note-actions';const remove=iconButton('remove','移入最近删除');actions.append(remove);row.append(handle,content,actions);
      toggleButton.onclick=async()=>{await toggle(note.id,toggleButton);if(drag)pendingRefresh=true;else render();};remove.onclick=()=>removeNote(note.id);handle.onpointerdown=e=>startSort(e,note.id,handle);
      handle.onkeydown=e=>{if(['ArrowUp','ArrowDown'].includes(e.key)){e.preventDefault();keyboardSort(note.id,e.key==='ArrowUp'?-1:1);}};
    }
    row.querySelector('.ink').textContent=note.text;const toggleButton=row.querySelector('.note-toggle');if(!busy.has(note.id))toggleButton.setAttribute('aria-pressed',String(note.done));
    const date=row.querySelector('.date-caption');date.textContent=dateCaption(note.date)||'未安排日期';date.setAttribute('aria-label',`${note.text}：${date.textContent}，点击修改日期`);date.classList.toggle('overdue',!!note.date&&!note.done&&note.date<localDate());date.dataset.tone=!note.date?'neutral':note.date===localDate()?'sage':note.date<localDate()?'rose':Number(note.date.slice(-2))%2?'blue':'lavender';date.title=note.date?`${note.date} · 点击修改`:'点击安排日期';
    list.append(row);
  }
  animateOrder(before);count();renderTrash();
}
function renderTrash(){const target=document.querySelector('#trash-list');target.replaceChildren();for(const note of [...deleted].reverse()){const row=document.createElement('div');row.className='trash-item';const text=document.createElement('span');text.textContent=note.text;const restore=document.createElement('button');restore.textContent='恢复';restore.onclick=()=>restoreNote(note.id);row.append(text,restore);target.append(row);}document.querySelector('#trash-empty').hidden=deleted.length>0;}
async function refresh(){if(drag){pendingRefresh=true;return;}const seq=++revision;try{const data=await invoke('notes_snapshot');if(seq!==revision)return;if(drag){pendingRefresh=true;return;}notes=data.notes;deleted=data.deleted;render();}catch(err){error.textContent=`读取失败：${err}`;}}
async function toggle(id,button){
  if(busy.has(id))return;busy.add(id);const note=notes.find(n=>n.id===id);const done=!note.done;button.setAttribute('aria-pressed',String(done));
  try{await invoke('set_note_done',{id,done});const current=notes.find(n=>n.id===id);if(current)current.done=done;count();error.textContent='';}
  catch(err){button.setAttribute('aria-pressed',String(note.done));error.textContent=`未能保存：${err}`;}
  finally{busy.delete(id);}
}
const tab = document.querySelector('#bubble');
const widget=createNoteWidget({header:document.querySelector('header'),invoke,availableMonitors,cursorPosition,bubble:tab,surface:document.querySelector('#surface'),paper,content:document.querySelector('#paper-content'),onError:err=>{error.textContent=String(err);},onExpand:()=>autoCollapse.revealed(),onCollapse:()=>{autoCollapse.collapsed();document.querySelector('#date-editor').hidden=true;document.querySelector('#trash-panel').hidden=true;}});
const autoCollapse=createAutoCollapse({collapse:()=>widget.collapse(),blocked:()=>!!drag||widget.dragging||busy.size>0||!document.querySelector('#date-editor').hidden||!document.querySelector('#trash-panel').hidden});
let pinned=false;
try{pinned=localStorage.getItem('notes-pinned')==='true';}catch{}
const pin=document.querySelector('#pin');
function updatePin(){pin.setAttribute('aria-pressed',String(pinned));pin.title=pinned?'取消固定，恢复自动收起':'固定便签，关闭自动收起';pin.setAttribute('aria-label',pin.title);autoCollapse.setPinned(pinned);}
pin.onclick=()=>{pinned=!pinned;updatePin();try{localStorage.setItem('notes-pinned',String(pinned));}catch{}};
updatePin();
paper.addEventListener('pointerenter',()=>autoCollapse.enter());
paper.addEventListener('pointerleave',()=>autoCollapse.leave());
paper.addEventListener('keydown',()=>autoCollapse.activity());
paper.addEventListener('pointerup',()=>autoCollapse.activity());
function updateToday(){const now=new Date();const date=document.querySelector('#today-date');date.dateTime=localDate(now);date.textContent=`${now.getMonth()+1}月${now.getDate()}日 周${'日一二三四五六'[now.getDay()]}`;date.title=localDate(now);}
updateToday();
async function change(command,args){try{await invoke(command,args);await refresh();error.textContent='';return true;}catch(err){await refresh();error.textContent=`未能保存：${err}`;return false;}}
async function removeNote(id){if(busy.has(id))return;busy.add(id);try{if(await change('delete_note',{id})){lastDeleted=id;document.querySelector('#undo-bar').hidden=false;}}finally{busy.delete(id);}}
async function restoreNote(id){if(await change('restore_deleted_note',{id})){if(lastDeleted===id)document.querySelector('#undo-bar').hidden=true;}}
function editDate(id){dateId=id;const shortcuts=document.querySelector('#quick-dates');shortcuts.replaceChildren();for(const option of quickDates()){const button=document.createElement('button');button.textContent=option.label;button.title=option.date;button.onclick=()=>saveDate(option.date);shortcuts.append(button);}document.querySelector('#note-date').value=notes.find(n=>n.id===id)?.date||'';document.querySelector('#date-editor').hidden=false;document.querySelector('#note-date').focus();}
async function saveDate(date){const id=dateId;if(!id||busy.has(id))return;busy.add(id);try{if(await change('set_note_date',{id,date:date||null})){document.querySelector('#date-editor').hidden=true;dateId=null;}}finally{busy.delete(id);}}
document.querySelector('#date-save').onclick=()=>saveDate(document.querySelector('#note-date').value);document.querySelector('#date-clear').onclick=()=>saveDate(null);document.querySelector('#date-cancel').onclick=()=>{document.querySelector('#date-editor').hidden=true;dateId=null;};
document.querySelector('#note-date').onkeydown=e=>{if(e.key==='Enter'){e.preventDefault();saveDate(e.target.value);}};
document.querySelector('#undo').onclick=()=>lastDeleted&&restoreNote(lastDeleted);
document.querySelector('#trash-toggle').onclick=()=>{renderTrash();document.querySelector('#trash-panel').hidden=false;};document.querySelector('#trash-close').onclick=()=>document.querySelector('#trash-panel').hidden=true;
document.querySelector('#close').onclick=()=>widget.collapse();document.querySelector('#exit').onclick=async()=>{try{await invoke('close_notes');}catch(err){await widget.reveal();error.textContent=String(err);}};
async function keyboardSort(id,delta){if(drag)return;const ids=visibleNotes().map(n=>n.id),index=ids.indexOf(id),to=index+delta;if(to<0||to>=ids.length)return;const next=[...ids];[next[index],next[to]]=[next[to],next[index]];await change('reorder_notes',{ids:mergeVisibleOrder(notes.map(n=>n.id),next)});list.querySelector(`[data-id="${id}"] .drag-handle`)?.focus();}
function startSort(event,id,handle){
  if(event.button!==0||drag||busy.size)return;event.preventDefault();list.setPointerCapture(event.pointerId);
  const row=list.querySelector(`[data-id="${id}"]`),rect=row.getBoundingClientRect();drag={id,pointer:event.pointerId,handle,row,startY:event.clientY,startTop:rect.top,ghost:null};
  const move=e=>{if(!drag||e.pointerId!==drag.pointer)return;const state=drag;if(!state.ghost&&Math.abs(e.clientY-state.startY)<4)return;
    if(!state.ghost){const ghost=row.cloneNode(true);ghost.classList.add('note-ghost');ghost.classList.remove('dragging');Object.assign(ghost.style,{left:rect.left+'px',top:rect.top+'px',width:rect.width+'px'});ghost.setAttribute('aria-hidden','true');ghost.inert=true;document.body.append(ghost);state.ghost=ghost;row.classList.add('dragging');}
    state.ghost.style.transform=`translateY(${e.clientY-state.startY}px)`;
    const bounds=list.getBoundingClientRect();if(e.clientY<bounds.top+25)list.scrollTop-=10;else if(e.clientY>bounds.bottom-25)list.scrollTop+=10;
    const y=e.clientY-bounds.top+list.scrollTop;const next=[...list.children].find(n=>n!==row&&y<n.offsetTop+n.offsetHeight/2)||null;
    if(next!==row.nextElementSibling){const before=positions();list.insertBefore(row,next);animateOrder(before);}
  };
  const finish=async(e,cancelled=false)=>{if(!drag||e.pointerId!==drag.pointer)return;const state=drag;drag=null;list.removeEventListener('pointermove',move);list.removeEventListener('pointerup',end);list.removeEventListener('pointercancel',cancel);list.removeEventListener('lostpointercapture',cancel);
    if(state.ghost){
      if(!cancelled){
        const settle=reduced.matches?Promise.resolve():state.ghost.animate([{transform:getComputedStyle(state.ghost).transform},{transform:`translateY(${row.getBoundingClientRect().top-state.startTop}px)`}],{duration:150,easing:'cubic-bezier(.22,1,.36,1)',fill:'forwards'}).finished.catch(()=>{});
        await Promise.all([settle,change('reorder_notes',{ids:mergeVisibleOrder(notes.map(n=>n.id),[...list.children].map(n=>Number(n.dataset.id)))})]);
      }
      state.ghost.remove();row.classList.remove('dragging');if(cancelled)render();
    }
    if(pendingRefresh){pendingRefresh=false;await refresh();}
  };
  const end=e=>finish(e),cancel=e=>finish(e,true);list.addEventListener('pointermove',move);list.addEventListener('pointerup',end);list.addEventListener('pointercancel',cancel);list.addEventListener('lostpointercapture',cancel);drag.cancel=()=>finish({pointerId:drag.pointer},true);
}
document.addEventListener('keydown',event=>{if(event.isComposing||event.key!=='Escape')return;event.preventDefault();if(drag){drag.cancel();return;}if(!document.querySelector('#date-editor').hidden){document.querySelector('#date-editor').hidden=true;dateId=null;return;}if(!document.querySelector('#trash-panel').hidden){document.querySelector('#trash-panel').hidden=true;return;}widget.collapse();});
async function init(){await win.listen('notes-open',async()=>{await refresh();await widget.reveal();});await win.listen('notes-changed',()=>{refresh();widget.pulse();});await win.listen('notes-exit',()=>widget.exit().catch(()=>{}));await refresh();await widget.init();}
init().catch(err=>{error.textContent=`便签未能初始化：${err}`;tab.title=String(err);});

for(const button of document.querySelectorAll('[data-view]'))button.onclick=()=>{
  if(drag)return;agendaView=button.dataset.view;
  for(const item of document.querySelectorAll('[data-view]'))item.setAttribute('aria-pressed',String(item===button));
  document.querySelector('#date-editor').hidden=true;dateId=null;render();
};
let calendarDay=localDate();
setInterval(()=>{const day=localDate();if(day!==calendarDay){calendarDay=day;updateToday();if(drag)pendingRefresh=true;else render();}},30000);

export function installStoryLibrary({invoke,current,onChanged,onError}) {
 const $=s=>document.querySelector(s);let pending=null;
 const message=(id,error)=>{$(id).textContent=String(error);};
 async function showPending(){try{const data=await invoke('story_delete_pending');if(!data)return;pending=data;$('#delete-story-description').textContent=`“${data.title}” · 共 ${data.turns} 轮`;message('#delete-story-status','');if(!$('#delete-story-dialog').open)$('#delete-story-dialog').showModal();}catch(e){onError(e);}}
 $('#delete-story').onclick=async()=>{const data=current();if(!data?.story||data.job.running)return;$('#delete-story').disabled=true;try{await invoke('story_delete_prepare',{id:data.story.id,revision:data.story.revision});await showPending();}catch(e){onError(e);}finally{$('#delete-story').disabled=!current()?.story||!!current()?.job.running;}};
 async function cancel(){const token=pending?.token;pending=null;$('#delete-story-dialog').close();if(token)try{await invoke('story_delete_cancel',{token});}catch(e){onError(e);}}
 $('#delete-story-cancel').onclick=cancel;
 $('#delete-story-dialog').oncancel=event=>{event.preventDefault();cancel();};
 $('#delete-story-form').onsubmit=async event=>{event.preventDefault();if(!pending)return;$('#delete-story-confirm').disabled=true;try{const result=await invoke('story_delete_confirm',{token:pending.token});pending=null;$('#delete-story-dialog').close();await onChanged(result.active,{clearInput:true});}catch(e){message('#delete-story-status',e);}finally{$('#delete-story-confirm').disabled=false;}};
 async function loadTrash(){const data=await invoke('story_trash');const root=$('#story-trash-list');root.replaceChildren();if(!data.stories.length){root.textContent='没有已删除的故事。';return;}root.textContent='';for(const story of data.stories){const row=document.createElement('div');row.className='trash-story';const label=document.createElement('span');label.textContent=`${story.title} · ${story.turns} 轮`;const restore=document.createElement('button');restore.type='button';restore.textContent='恢复';restore.onclick=async()=>{restore.disabled=true;try{const result=await invoke('story_restore',{id:story.id});await onChanged(result.active);await loadTrash();}catch(e){message('#story-trash-status',e);restore.disabled=false;}};row.append(label,restore);root.append(row);}}
 $('#story-trash').onclick=async()=>{message('#story-trash-status','');if(!$('#story-trash-dialog').open)$('#story-trash-dialog').showModal();try{await loadTrash();}catch(e){message('#story-trash-status',e);}};
 $('#story-trash-close').onclick=()=>$('#story-trash-dialog').close();
 return {showPending};
}

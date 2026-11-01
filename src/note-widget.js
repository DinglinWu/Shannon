import { compactRect,expandedRect,snapAnchor,monitorFor,areaOf,COMPACT,dockAppearance,draggedExpanded } from './note-widget-model.js';
const frame=()=>new Promise(resolve=>requestAnimationFrame(resolve));
export function createNoteWidget({invoke,availableMonitors,cursorPosition,bubble,surface,paper,content,header,onError,onCollapse,onExpand}) {
  const reduced=matchMedia('(prefers-reduced-motion: reduce)');
  let anchor={x:0,y:0},monitors=[],open=false,visible=false,exiting=false,sequence=0,life=0,queue=Promise.resolve(),motions=[];
  let drag=null,suppressClick=false,liveBounds=null,masked=false;
  function edge(){const shape=dockAppearance(anchor,monitors);document.body.dataset.edge=shape.edge;document.documentElement.style.setProperty('--dock',String(shape.amount));}
  function layout(rect,seq=sequence){
    queue=queue.catch(()=>{}).then(async()=>{
      if(seq!==sequence)return;
      // Retain a painted large viewport when compact; the native region clips
      // both pixels and mouse hits. Moving it never shrinks the WebView to 64px.
      const compact=rect.width===Math.round(COMPACT*(rect.scale||1));
      if(compact&&masked){
        const bounds=expandedRect(anchor,monitors);
        document.documentElement.style.setProperty('--anchor-fx',bounds.fx);
        document.documentElement.style.setProperty('--anchor-fy',bounds.fy);
        await invoke('notes_widget_bounds',{x:Math.round(bounds.x),y:Math.round(bounds.y),width:Math.round(bounds.width),height:Math.round(bounds.height)});
        liveBounds={...bounds};
        await maskIcon(rect);return;
      }
      await invoke('notes_widget_bounds',{x:Math.round(rect.x),y:Math.round(rect.y),width:Math.round(rect.width),height:Math.round(rect.height)});
      liveBounds={...rect};
    });return queue;
  }
  async function maskIcon(compact){
    await invoke('notes_widget_mask',{x:Math.round(compact.x-liveBounds.x),y:Math.round(compact.y-liveBounds.y),size:compact.width});
    masked=true;
  }

  function cancel(){for(const m of motions)m.cancel();motions=[];}
  function animate(element,keyframes,options={}){if(reduced.matches)return Promise.resolve();const m=element.animate(keyframes,{duration:380,easing:'cubic-bezier(.22,1,.36,1)',fill:'backwards',...options});motions.push(m);return m.finished;}
  function bubbleClip(){const r=bubble.getBoundingClientRect();return `inset(${Math.max(0,r.top)}px ${Math.max(0,innerWidth-r.right)}px ${Math.max(0,innerHeight-r.bottom)}px ${Math.max(0,r.left)}px round ${document.body.dataset.edge==='free'?24:14}px)`;}
  async function change(next,allowExit=false){
    if(exiting&&!allowExit)return;if(open===next&&visible===next)return;open=next;const seq=++sequence;
    const from=visible?getComputedStyle(surface).clipPath:null,fromOpacity=getComputedStyle(bubble).opacity;
    cancel();paper.inert=!next;bubble.setAttribute('aria-expanded',String(next));
    try{
      if(next){
        monitors=await availableMonitors();const rect=expandedRect(anchor,monitors);
        document.documentElement.style.setProperty('--anchor-fx',rect.fx);
        document.documentElement.style.setProperty('--anchor-fy',rect.fy);
        await layout(rect,seq);if(seq!==sequence)return;
        if(masked){await invoke('notes_widget_mask',{x:null,y:null,size:null});masked=false;if(seq!==sequence)return;}
        const start=from||bubbleClip(),end='inset(8px 8px 8px 8px round 14px)';
        surface.style.clipPath=start;surface.hidden=false;visible=true;content.style.opacity='0';
        await frame();await frame();if(seq!==sequence)return;
        surface.style.clipPath=end;bubble.style.opacity='0';bubble.inert=true;content.style.opacity='1';
        await Promise.all([animate(surface,[{clipPath:start},{clipPath:end}],{duration:420}),animate(bubble,[{opacity:fromOpacity},{opacity:0}],{duration:180}),animate(content,[{opacity:0,transform:'translateY(5px)'},{opacity:1,transform:'none'}],{duration:260,delay:110})]);if(seq===sequence)onExpand?.();
      }else{
        const start=from||bubbleClip(),end=bubbleClip();surface.style.clipPath=end;content.style.opacity='0';bubble.style.opacity='1';bubble.inert=false;
        await Promise.all([animate(surface,[{clipPath:start},{clipPath:end}],{duration:360}),animate(content,[{opacity:1},{opacity:0}],{duration:150}),animate(bubble,[{opacity:fromOpacity},{opacity:1}],{duration:180,delay:140})]);
        if(seq!==sequence)return;surface.hidden=true;visible=false;onCollapse?.();
        await frame();await frame();if(seq!==sequence)return;
        const compact=compactRect(anchor,monitors);
        try{queue=queue.catch(()=>{}).then(()=>seq===sequence?maskIcon(compact):undefined);await queue;}catch(error){
          // Other platforms retain the bounded resize path. Windows failures
          // also recover to a usable compact window instead of blocking input.
          masked=false;await layout(compact,seq);
        }
      }
    }catch(error){if(error.name!=='AbortError'&&seq===sequence)onError(error);}
  }
  const ready=(async()=>{const [state,list]=await Promise.all([invoke('notes_widget_state'),availableMonitors()]);monitors=list;anchor=snapAnchor(state.anchor,monitors);edge(anchor.edge);await layout(compactRect(anchor,monitors));return state;})();
  async function reveal(){const ticket=++life;await ready;if(ticket!==life)return;await finishDrag(true);if(ticket!==life)return;const wasExiting=exiting;exiting=false;if(wasExiting){cancel();bubble.style.opacity=open?'0':'1';}await change(true);}
  async function collapse(){await ready;await finishDrag(true);await change(false);}
  async function finishDrag(cancelled=false){
    const current=drag;if(!current)return;drag=null;document.body.classList.remove('drag-ready');
    if(!current.armed){suppressClick=current.moved;return;}
    suppressClick=true;
    try{
      await current.initialized;await current.task;await queue;
      if(current.expanded){
        if(!cancelled){const moved=draggedExpanded(await cursorPosition(),current,monitors);anchor=moved.anchor;await layout(moved.rect);}
        edge();await invoke('notes_widget_save',{x:Math.round(anchor.x),y:Math.round(anchor.y)});return;
      }
      if(!cancelled){const point=await cursorPosition();const m=monitorFor(point,monitors),scale=areaOf(m).scale;anchor=compactRect({x:point.x-current.u*COMPACT*scale,y:point.y-current.v*COMPACT*scale},[m]);await layout(anchor);}
      const target=snapAnchor(anchor,monitors),start={...anchor};edge(target.edge);
      const t0=performance.now();
      do{const t=reduced.matches?1:Math.min(1,(performance.now()-t0)/110),ease=1-(1-t)**3;anchor={...target,x:start.x+(target.x-start.x)*ease,y:start.y+(target.y-start.y)*ease};edge();await layout(anchor);if(t===1)break;await frame();}while(true);
      anchor=target;edge();await invoke('notes_widget_save',{x:Math.round(anchor.x),y:Math.round(anchor.y)});
    }catch(error){onError(error);}
  }
  bubble.addEventListener('pointerdown',event=>{
    if(event.button!==0||open||exiting||drag)return;suppressClick=false;bubble.setPointerCapture(event.pointerId);
    const first=cursorPosition();const current=drag={id:event.pointerId,startX:event.clientX,startY:event.clientY,moved:false,armed:false,u:.5,v:.5,task:Promise.resolve()};
    current.initialized=(async()=>{try{await ready;const p=await first;const r=compactRect(anchor,monitors);current.u=Math.max(0,Math.min(1,(p.x-anchor.x)/r.width));current.v=Math.max(0,Math.min(1,(p.y-anchor.y)/r.height));}catch(error){onError(error);}})();
  });
  const movePointer=event=>{
    const current=drag;if(!current||current.id!==event.pointerId)return;
    if(Math.hypot(event.clientX-current.startX,event.clientY-current.startY)>3){current.moved=true;current.armed=true;document.body.classList.add('drag-ready');}
    if(!current.armed||current.moving)return;current.moving=true;
    current.task=(async()=>{try{await current.initialized;await frame();if(drag!==current)return;const p=await cursorPosition();if(drag!==current)return;if(current.expanded){const moved=draggedExpanded(p,current,monitors);anchor=moved.anchor;edge();await layout(moved.rect);return;}const m=monitorFor(p,monitors),scale=areaOf(m).scale;anchor=compactRect({x:p.x-current.u*COMPACT*scale,y:p.y-current.v*COMPACT*scale},[m]);edge(snapAnchor(anchor,monitors).edge);await layout(anchor);}catch(error){onError(error);}finally{current.moving=false;}})();
  };
  bubble.addEventListener('pointermove',movePointer);
  bubble.addEventListener('pointerup',event=>{if(drag?.id===event.pointerId){if(drag.armed)suppressClick=true;finishDrag();}});
  bubble.addEventListener('pointercancel',()=>finishDrag(true));
  bubble.addEventListener('lostpointercapture',()=>{if(drag)finishDrag(true);});
  bubble.addEventListener('click',()=>{if(suppressClick){suppressClick=false;return;}reveal();});
  bubble.addEventListener('contextmenu',event=>event.preventDefault());
  header?.addEventListener('pointerdown',event=>{
    if(event.button!==0||!open||exiting||drag||event.target.closest('button,input,select,a'))return;
    event.preventDefault();header.setPointerCapture(event.pointerId);
    const rect=liveBounds||expandedRect(anchor,monitors),first=cursorPosition();
    const current=drag={id:event.pointerId,expanded:true,startX:event.clientX,startY:event.clientY,moved:false,armed:false,u:0,v:0,fx:rect.fx??0,fy:rect.fy??0,task:Promise.resolve()};
    current.initialized=(async()=>{await ready;const p=await first;current.u=(p.x-rect.x)/rect.width;current.v=(p.y-rect.y)/rect.height;})();
  });
  header?.addEventListener('pointermove',movePointer);
  header?.addEventListener('pointerup',event=>{if(drag?.id===event.pointerId)finishDrag();});
  header?.addEventListener('pointercancel',()=>finishDrag(true));
  header?.addEventListener('lostpointercapture',()=>{if(drag?.expanded)finishDrag(true);});

  return {init:async()=>{const state=await ready;if(state.reveal)await reveal();},reveal,collapse,get expanded(){return open;},get dragging(){return !!drag;},
    async exit(){const ticket=++life;exiting=true;await ready;await finishDrag(true);if(ticket!==life)return;await change(false,true);if(ticket!==life)return;bubble.style.opacity='0';try{await animate(bubble,[{opacity:1,transform:'scale(1)'},{opacity:0,transform:'scale(.25)'}],{duration:160});}catch(error){if(error.name!=='AbortError')throw error;}},
    pulse(){if(exiting){life++;exiting=false;cancel();bubble.style.opacity='1';change(false);return;}if(!open&&!reduced.matches)bubble.animate([{filter:'brightness(1)'},{filter:'brightness(1.12)'},{filter:'brightness(1)'}],{duration:500});}
  };
}

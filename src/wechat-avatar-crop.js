const clamp=(n,min,max)=>Math.max(min,Math.min(max,n));
export function createCrop(width,height){
 if(!Number.isFinite(width)||!Number.isFinite(height)||width<=0||height<=0)throw new Error('图片尺寸无效');
 return {width,height,x:width/2,y:height/2,zoom:1};
}
export function cropRect(state){const side=Math.min(state.width,state.height)/state.zoom;return {x:state.x-side/2,y:state.y-side/2,side};}
export function zoomCrop(state,value){const zoom=clamp(Number.isFinite(value)?value:1,1,4),half=Math.min(state.width,state.height)/zoom/2;return {...state,zoom,x:clamp(state.x,half,state.width-half),y:clamp(state.y,half,state.height-half)};}
export function panCrop(state,dx,dy,displaySize){if(!Number.isFinite(displaySize)||displaySize<=0)return state;const ratio=cropRect(state).side/displaySize;return zoomCrop({...state,x:state.x-dx*ratio,y:state.y-dy*ratio},state.zoom);}
export function mountAvatarCrop(root,bitmap){
 let state=createCrop(bitmap.width,bitmap.height),frame=0,drag=null,disposed=false;
 root.replaceChildren();
 const stage=document.createElement('div');stage.className='avatar-crop-stage';
 const canvas=document.createElement('canvas');canvas.width=canvas.height=256;canvas.tabIndex=0;canvas.setAttribute('aria-label','头像裁剪区域：拖动图片，方向键微调位置');stage.append(canvas);
 const controls=document.createElement('div');controls.className='avatar-crop-controls';
 const label=document.createElement('label');label.textContent='缩放';const slider=document.createElement('input');slider.type='range';slider.min=1;slider.max=4;slider.step=.01;slider.value=1;slider.setAttribute('aria-label','头像缩放比例');label.append(slider);
 const reset=document.createElement('button');reset.type='button';reset.textContent='重置';controls.append(label,reset);root.append(stage,controls);
 const draw=()=>{frame=0;if(disposed)return;const {x,y,side}=cropRect(state),ctx=canvas.getContext('2d');ctx.clearRect(0,0,256,256);ctx.imageSmoothingEnabled=true;ctx.imageSmoothingQuality='high';ctx.drawImage(bitmap,x,y,side,side,0,0,256,256);};
 const schedule=()=>{if(!frame)frame=requestAnimationFrame(draw);};
 const scale=value=>{state=zoomCrop(state,value);slider.value=String(state.zoom);schedule();};
 slider.oninput=()=>scale(Number(slider.value));reset.onclick=()=>{state=createCrop(bitmap.width,bitmap.height);slider.value=1;schedule();};
 canvas.onpointerdown=e=>{if(e.button!==0||drag)return;e.preventDefault();drag={id:e.pointerId,x:e.clientX,y:e.clientY};canvas.setPointerCapture(e.pointerId);canvas.focus();stage.classList.add('dragging');};
 canvas.onpointermove=e=>{if(!drag||e.pointerId!==drag.id)return;state=panCrop(state,e.clientX-drag.x,e.clientY-drag.y,canvas.getBoundingClientRect().width);drag.x=e.clientX;drag.y=e.clientY;schedule();};
 const release=e=>{if(drag?.id!==e.pointerId)return;drag=null;stage.classList.remove('dragging');if(canvas.hasPointerCapture(e.pointerId))canvas.releasePointerCapture(e.pointerId);};
 canvas.onpointerup=release;canvas.onpointercancel=release;canvas.onlostpointercapture=()=>{drag=null;stage.classList.remove('dragging');};
 canvas.addEventListener('wheel',e=>{e.preventDefault();scale(state.zoom*Math.exp(-e.deltaY*.0015));},{passive:false});
 canvas.onkeydown=e=>{const step=e.shiftKey?20:4;const delta={ArrowLeft:[-step,0],ArrowRight:[step,0],ArrowUp:[0,-step],ArrowDown:[0,step]}[e.key];if(delta){e.preventDefault();state=panCrop(state,...delta,canvas.getBoundingClientRect().width);schedule();}};
 draw();
 return {exportData(){if(disposed)throw new Error('裁剪界面已关闭');if(frame){cancelAnimationFrame(frame);frame=0;}draw();return canvas.toDataURL('image/png');},dispose(){disposed=true;if(frame)cancelAnimationFrame(frame);bitmap.close();root.replaceChildren();}};
}

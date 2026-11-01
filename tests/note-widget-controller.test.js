import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import * as model from '../src/note-widget-model.js';
const source=readFileSync(new URL('../src/note-widget.js',import.meta.url),'utf8').replace(/^import[^\n]+\n/,'').replace('export function','function');
function setup(reduced=true,maskSupported=true){
  const timers=new Map(),bounds=[],masks=[],errors=[],animations=[];let cursor={x:700,y:200};
  function element(){const listeners=new Map();return {animate(frames,options){animations.push({frames,options});return{finished:Promise.resolve(),cancel(){}};},style:{opacity:'1',clipPath:''},hidden:true,inert:false,setAttribute(){},setPointerCapture(){},addEventListener:(n,fn)=>listeners.set(n,fn),emit:(n,e={})=>listeners.get(n)?.(e),getBoundingClientRect:()=>({top:8,left:16,right:64,bottom:56})};}
  const bubble=element(),surface=element(),paper=element(),content=element(),header=element();
  const ctx={...model,document:{body:{dataset:{},classList:{add(){},remove(){}}},documentElement:{style:{setProperty(){}}}},matchMedia:()=>({matches:reduced}),requestAnimationFrame:fn=>queueMicrotask(fn),getComputedStyle:e=>e.style,innerWidth:380,innerHeight:460,performance,clearTimeout:id=>timers.delete(id),setTimeout:fn=>{const id=Symbol();timers.set(id,fn);return id;},Promise};
  vm.createContext(ctx);vm.runInContext(source+'\nthis.create=createNoteWidget;',ctx);
  const widget=ctx.create({bubble,surface,paper,content,header,onError:e=>errors.push(e),availableMonitors:async()=>[{scaleFactor:1,workArea:{position:{x:0,y:0},size:{width:1000,height:800}}}],cursorPosition:async()=>cursor,invoke:async(name,args)=>{if(name==='notes_widget_state')return{anchor:{x:936,y:100},reveal:false};if(name==='notes_widget_bounds')bounds.push(args);if(name==='notes_widget_mask'){if(!maskSupported)throw new Error('unsupported');masks.push(args);}}});
  return {widget,bubble,header,surface,paper,bounds,masks,errors,timers,animations,setCursor:p=>cursor=p};
}
test('startup stays compact; hover does not expand; repeated reveal and collapse finish compact',async()=>{
  const s=setup();await s.widget.init();assert.equal(s.widget.expanded,false);assert.equal(s.bounds.at(-1).width,64);
  s.bubble.emit('pointerenter');assert.equal(s.widget.expanded,false);
  await s.widget.reveal();await s.widget.reveal();assert.equal(s.bubble.style.opacity,'0');assert.equal(s.paper.inert,false);
  await s.widget.collapse();assert.equal(s.surface.hidden,true);assert.equal(s.paper.inert,true);assert.equal(s.bounds.at(-1).width,380);assert.equal(s.masks.at(-1).size,64);
  assert.deepEqual(s.errors,[]);
});
test('press and move immediately drags and snaps without opening the panel',async()=>{
  const s=setup();await s.widget.init();s.setCursor({x:968,y:132});s.bubble.emit('pointerdown',{button:0,pointerId:1,clientX:32,clientY:32});
  s.setCursor({x:450,y:350});s.bubble.emit('pointermove',{pointerId:1,clientX:40,clientY:35});
  await new Promise(r=>setImmediate(r));s.bubble.emit('pointerup',{pointerId:1});await new Promise(r=>setImmediate(r));s.bubble.emit('click');
  assert.equal(s.widget.expanded,false);assert.equal(s.bounds.at(-1).x,418);assert.equal(s.bounds.at(-1).y,318);assert.deepEqual(s.errors,[]);
});
test('an immediate reopen cancels a pending exit and a later addition revives the icon',async()=>{
  const s=setup();await s.widget.init();await s.widget.reveal();await Promise.all([s.widget.exit(),s.widget.reveal()]);
  assert.equal(s.widget.expanded,true);assert.equal(s.bubble.style.opacity,'0');
  await s.widget.exit();assert.equal(s.bubble.style.opacity,'0');s.widget.pulse();await new Promise(r=>setImmediate(r));
  assert.equal(s.bubble.style.opacity,'1');assert.equal(s.widget.expanded,false);assert.deepEqual(s.errors,[]);
});

test('a short click or tiny pointer jitter still opens the panel',async()=>{
 const s=setup();await s.widget.init();s.setCursor({x:968,y:132});
 s.bubble.emit('pointerdown',{button:0,pointerId:1,clientX:32,clientY:32});
 s.bubble.emit('pointermove',{pointerId:1,clientX:33,clientY:33});
 s.bubble.emit('pointerup',{pointerId:1});s.bubble.emit('click');
 await new Promise(r=>setImmediate(r));assert.equal(s.widget.expanded,true);assert.deepEqual(s.errors,[]);
});

test('expanded header dragging keeps the panel open and collapse follows its new position',async()=>{
 const s=setup();await s.widget.init();await s.widget.reveal();s.setCursor({x:650,y:120});
 s.header.emit('pointerdown',{button:0,pointerId:2,clientX:30,clientY:20,target:{closest:()=>null},preventDefault(){}});
 s.setCursor({x:430,y:220});s.header.emit('pointermove',{pointerId:2,clientX:40,clientY:30});
 await new Promise(r=>setImmediate(r));s.header.emit('pointerup',{pointerId:2});await new Promise(r=>setImmediate(r));
 assert.equal(s.widget.expanded,true);assert.equal(s.bounds.at(-1).width,380);assert.equal(s.bounds.at(-1).x,400);assert.equal(s.bounds.at(-1).y,200);
 await s.widget.collapse();assert.equal(s.bounds.at(-1).width,380);assert.equal(s.masks.at(-1).size,64);assert.equal(s.bounds.at(-1).x+s.masks.at(-1).x,716);assert.equal(s.bounds.at(-1).y+s.masks.at(-1).y,200);assert.deepEqual(s.errors,[]);
});
test('expanded header buttons do not start a window drag',async()=>{
 const s=setup();await s.widget.init();await s.widget.reveal();const count=s.bounds.length;
 s.header.emit('pointerdown',{button:0,pointerId:2,target:{closest:()=>({})}});
 s.header.emit('pointermove',{pointerId:2,clientX:80,clientY:80});await new Promise(r=>setImmediate(r));assert.equal(s.bounds.length,count);
});

test('delayed icon and content fades hold the transparent first frame instead of flashing visible',async()=>{
 const s=setup(false);await s.widget.init();await s.widget.reveal();await s.widget.collapse();
 const delayed=s.animations.filter(a=>a.options.delay>0);
 assert.equal(delayed.length,2);
 for(const animation of delayed){assert.equal(animation.options.fill,'backwards');assert.equal(Number(animation.frames[0].opacity),0);}
 assert.equal(s.bubble.style.opacity,'1');assert.equal(s.surface.hidden,true);assert.deepEqual(s.errors,[]);
});

test('native region keeps the same painted viewport on collapse and clears before reopening',async()=>{
 const s=setup();await s.widget.init();await s.widget.reveal();const count=s.bounds.length;
 await s.widget.collapse();assert.equal(s.bounds.length,count);assert.equal(s.masks.at(-1).size,64);
 await s.widget.reveal();assert.equal(s.masks.at(-1).size,null);assert.equal(s.surface.hidden,false);assert.deepEqual(s.errors,[]);
});
test('unsupported native clipping falls back to a compact window without leaving the panel open',async()=>{
 const s=setup(true,false);await s.widget.init();await s.widget.reveal();await s.widget.collapse();
 assert.equal(s.bounds.at(-1).width,64);assert.equal(s.surface.hidden,true);assert.deepEqual(s.errors,[]);
});

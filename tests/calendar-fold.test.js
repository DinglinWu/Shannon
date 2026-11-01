import test from 'node:test';
import assert from 'node:assert/strict';
import { settleCalendarFold } from '../src/calendar-fold.js';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';

test('fold preserves both scroll offsets when reparenting resets a scroll container',async()=>{
  const source=readFileSync(new URL('../src/calendar-fold.js',import.meta.url),'utf8').replace(/export /g,'');
  const result={scrollTop:180,scrollLeft:24,style:{},before(){}};
  const resets=[];
  const reset=()=>{result.scrollTop=0;result.scrollLeft=0;resets.push(true);};
  const element=()=>({style:{},append(...children){if(children.includes(result))reset();},remove(){},animate(){return {pause(){},play(){},cancel(){},finished:Promise.resolve()};}});
  const context={document:{createElement:element,createComment:()=>({replaceWith:reset}),timeline:{currentTime:0}},Promise};
  vm.createContext(context);vm.runInContext(source,context);
  await context.calendarFold({main:element(),chat:{classList:{add(){}}},front:{style:{width:'640px'}},result,dock:{...element(),offsetHeight:60},height:300,offsetX:0,dockY:300,sourceWidth:640,width:640,duration:0});
  assert.equal(resets.length,2);assert.equal(result.scrollTop,180);assert.equal(result.scrollLeft,24);
});

test('launcher keeps compact dimensions during paints before and after native viewport resize',async()=>{
  const source=readFileSync(new URL('../src/main.js',import.meta.url),'utf8').replace(/\r\n/g,'\n');
  const start=source.indexOf('    await settleCalendarFold({');
  const end=source.indexOf('\n  }\n  queryInput.focus();',start);
  let viewport=1000;const paints=[];
  const classes={add(){},remove(){}};
  const appEl={classList:classes,style:{width:'640px',height:'700px',removeProperty(){}}};
  const ctx={appEl,COLLAPSED_WIDTH:640,targetH:240,chatViewEl:{style:{},classList:classes},settingsBtn:{classList:classes},freezeChatWidth(){},endMorph(){},scheduleResize(){},
    applySize:async()=>{viewport=640;},
    settleCalendarFold:options=>settleCalendarFold({...options,paint:async()=>{paints.push({width:appEl.style.width||`${viewport}px`,height:appEl.style.height});}})};
  vm.createContext(ctx);
  await vm.runInContext(`(async()=>{${source.slice(start,end)}})()`,ctx);
  assert.deepEqual(paints,[{width:'640px',height:'240px'},{width:'640px',height:'240px'}]);
  assert.equal(appEl.style.width,'');assert.equal(appEl.style.height,'');assert.equal(viewport,640);
});

test('native resize waits for the final surface; release waits for resize and repaint', async () => {
  const events = [];
  let finishResize;
  const pending = settleCalendarFold({
    commit: () => events.push('commit'),
    paint: async () => { events.push('paint'); },
    resize: () => { events.push('resize'); return new Promise(resolve => { finishResize = resolve; }); },
    release: () => events.push('release'),
  });
  await Promise.resolve();
  assert.deepEqual(events, ['commit', 'paint', 'resize']);
  finishResize();
  await pending;
  assert.deepEqual(events, ['commit', 'paint', 'resize', 'paint', 'release']);
});

test('a failed native resize still releases the animation lock', async () => {
  let released = false;
  await assert.rejects(settleCalendarFold({
    commit() {}, paint: async () => {},
    resize: async () => { throw new Error('resize failed'); },
    release: () => { released = true; },
  }), /resize failed/);
  assert.equal(released, true);
});

test('fold hinge stays on the moving input top while the answer lands below it',async()=>{
 const source=readFileSync(new URL('../src/calendar-fold.js',import.meta.url),'utf8').replace(/export /g,'');
 const nodes=[];const element=()=>{const node={style:{},append(){},remove(){},animate(frames){node.frames=frames;return{pause(){},play(){},cancel(){},finished:Promise.resolve()};}};nodes.push(node);return node;};
 const ctx={document:{createElement:element,createComment:()=>({replaceWith(){}}),timeline:{currentTime:0}},Promise};vm.createContext(ctx);vm.runInContext(source,ctx);
 await ctx.calendarFold({main:element(),chat:{classList:{add(){}}},front:{style:{width:'640px'}},result:{style:{},scrollTop:0,scrollLeft:0,before(){}},dock:{...element(),offsetHeight:60},height:300,dockY:310,offsetX:20,sourceWidth:800,width:640,duration:0});
 const leaf=nodes.find(n=>n.className==='calendar-leaf'),back=nodes.find(n=>n.className==='calendar-back');
 assert.match(leaf.frames[0].transform,/translate\(20px, 10px\)/);
 assert.match(leaf.frames[1].transform,/translate\(0, -300px\)/);
 assert.equal(back.style.transform,'rotateX(180deg) translateY(60px)');
});

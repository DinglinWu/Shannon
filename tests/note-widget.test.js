import test from 'node:test';
import assert from 'node:assert/strict';
import {compactRect,expandedRect,snapAnchor,dateCaption,localDate,orderedBefore,agendaNotes,mergeVisibleOrder,quickDates,dockAppearance} from '../src/note-widget-model.js';
const monitor=(x,y,width,height,scaleFactor=1)=>({workArea:{position:{x,y},size:{width,height}},scaleFactor});
test('collapsed window is 64 logical pixels and clamps within the work area',()=>{
  assert.deepEqual(compactRect({x:1900,y:1100},[monitor(0,0,1920,1040)]),{x:1856,y:976,width:64,height:64,scale:1});
  const r=compactRect({x:-100,y:500},[monitor(-2560,0,2560,1400,2)]);
  assert.equal(r.width,128);assert.equal(r.x,-128);
});
test('only nearby edges snap; central placement stays a free sphere',()=>{
  const m=[monitor(0,0,1920,1040)];
  assert.equal(snapAnchor({x:1000,y:400},m).edge,'free');
  const right=snapAnchor({x:1840,y:400},m);assert.equal(right.edge,'right');assert.equal(right.x,1856);
  assert.equal(snapAnchor({x:300,y:20},m).edge,'top');
  assert.equal(snapAnchor({x:300,y:880},m).edge,'free');
  assert.equal(snapAnchor({x:-2350,y:400},[monitor(-2560,0,2560,1400,2)]).edge,'free');
});
test('expansion stays on the same monitor and preserves the icon screen position',()=>{
  const monitors=[monitor(-2560,-100,2560,1400,2),monitor(0,0,1920,1040)];
  for(const point of [{x:-128,y:500},{x:-2500,y:-80},{x:1856,y:976},{x:900,y:400}]){
    const c=compactRect(point,monitors),r=expandedRect(c,monitors);
    assert.ok(Math.abs((r.x+(r.width-c.width)*r.fx)-c.x)<.01);
    assert.ok(Math.abs((r.y+(r.height-c.height)*r.fy)-c.y)<.01);
    assert.ok(r.fx>=0&&r.fx<=1&&r.fy>=0&&r.fy<=1);
    const a=point.x<0?monitors[0].workArea:monitors[1].workArea;
    assert.ok(r.x>=a.position.x&&r.y>=a.position.y);
    assert.ok(r.x+r.width<=a.position.x+a.size.width&&r.y+r.height<=a.position.y+a.size.height);
  }
});
test('small screens still keep the complete expanded widget inside the work area',()=>{
  const r=expandedRect({x:130,y:130},[monitor(0,0,240,300)]);
  assert.equal(r.width,240);assert.equal(r.height,300);assert.equal(r.x,0);assert.equal(r.y,0);
});
test('calendar labels use local dates across month and year boundaries',()=>{
  const now=new Date(2026,11,31,23,50);
  assert.equal(localDate(now),'2026-12-31');assert.equal(dateCaption('2027-01-01',now),'明天');
  assert.equal(dateCaption('2026-12-31',now),'今天');assert.equal(dateCaption(null,now),'');
  assert.equal(dateCaption('2027-02-01',now),'2027年2月1日');
});
test('reordering retains every item and rejects stale targets',()=>{
  const ids=[1,2,3];assert.deepEqual(orderedBefore(ids,3,1),[3,1,2]);
  assert.deepEqual(orderedBefore(ids,1,null),[2,3,1]);assert.deepEqual(ids,[1,2,3]);
  assert.throws(()=>orderedBefore(ids,4,1));assert.throws(()=>orderedBefore(ids,1,4));
});

test('today excludes future dates and completed overdue items; future and all stay accessible',()=>{
  const notes=[{id:1,date:null},{id:2,date:'2026-10-28'},{id:3,date:'2026-10-29'},{id:4,date:'2026-10-27',done:false},{id:5,date:'2026-10-27',done:true}];
  const now=new Date(2026,9,28);
  assert.deepEqual(agendaNotes(notes,'today',now).map(n=>n.id),[1,2,4]);
  assert.deepEqual(agendaNotes(notes,'upcoming',now).map(n=>n.id),[3]);
  assert.equal(agendaNotes(notes,'all',now).length,5);
  assert.deepEqual(agendaNotes(notes,'today',new Date(2026,9,29)).map(n=>n.id),[1,2,3,4]);
});
test('sorting a filtered agenda preserves all hidden item slots',()=>{
  assert.deepEqual(mergeVisibleOrder([1,2,3,4,5],[5,1,3]),[5,2,1,4,3]);
  assert.throws(()=>mergeVisibleOrder([1,2],[1,1]));assert.throws(()=>mergeVisibleOrder([1,2],[3]));
});
test('quick dates cover today tomorrow the day after and five distinct weekdays across year end',()=>{
  const options=quickDates(new Date(2026,11,31,23,59));
  assert.deepEqual(options.slice(0,3).map(n=>n.date),['2026-12-31','2027-01-01','2027-01-02']);
  assert.equal(options.length,8);assert.equal(new Set(options.map(n=>n.date)).size,8);
  for(const item of options.slice(3)){const [y,m,d]=item.date.split('-').map(Number);assert.ok(![0,6].includes(new Date(y,m-1,d).getDay()));}
});
test('wider snap range scales with display density',()=>{
 assert.equal(snapAnchor({x:60,y:300},[monitor(0,0,1920,1040)]).edge,'left');
 assert.equal(snapAnchor({x:130,y:300},[monitor(0,0,2560,1400,2)]).edge,'left');
});

test('leaving an edge immediately becomes a complete sphere while snap range remains wide',()=>{
 const monitors=[monitor(0,0,1000,800)];
 const at=x=>dockAppearance({x,y:300},monitors);
 assert.equal(at(0).amount,1);
 for(const distance of [1,12,24,48,60]){assert.equal(at(distance).amount,0);assert.equal(at(distance).edge,'free');}
 assert.equal(snapAnchor({x:60,y:300},monitors).edge,'left');
 assert.equal(dockAppearance({x:1,y:300},[monitor(0,0,2000,1600,2)]).amount,0);
});

import test from 'node:test';
import assert from 'node:assert/strict';
import {createAutoCollapse} from '../src/note-auto-collapse.js';
function setup(){
  let closed=0,blocked=false,id=0;const timers=new Map();
  const auto=createAutoCollapse({collapse:()=>closed++,blocked:()=>blocked,setTimer:fn=>{timers.set(++id,fn);return id;},clearTimer:id=>timers.delete(id)});
  return {auto,get closed(){return closed;},get pending(){return timers.size;},block:value=>blocked=value,expire(){const entries=[...timers];timers.clear();for(const [,fn]of entries)fn();}};
}
test('leaving an expanded note schedules withdrawal and returning cancels it',()=>{
 const s=setup();s.auto.enter();s.auto.revealed();assert.equal(s.pending,0);
 s.auto.leave();assert.equal(s.pending,1);s.auto.enter();s.expire();assert.equal(s.closed,0);
 s.auto.leave();s.expire();assert.equal(s.closed,1);
});
test('pinning cancels pending collapse and unpinning restores it',()=>{
 const s=setup();s.auto.revealed();s.auto.setPinned(true);s.expire();assert.equal(s.closed,0);
 s.auto.setPinned(false);s.expire();assert.equal(s.closed,1);
});
test('editing dragging and saving postpone collapse until idle',()=>{
 const s=setup();s.block(true);s.auto.revealed();s.expire();assert.equal(s.closed,0);assert.equal(s.pending,1);
 s.block(false);s.auto.activity();s.expire();assert.equal(s.closed,1);
});
test('explicit collapse cancels the timer and later reopening starts a new grace period',()=>{
 const s=setup();s.auto.revealed();s.auto.collapsed();s.expire();assert.equal(s.closed,0);
 s.auto.revealed();s.expire();assert.equal(s.closed,1);
});

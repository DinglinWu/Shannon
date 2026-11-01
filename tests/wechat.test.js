import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
const source=readFileSync(new URL('../src/wechat.js',import.meta.url),'utf8');
function setup(){
 let resolve,reject;const rows=[],heading={textContent:'小林',classList:{add(){},remove(){}}};
 const state={liveReplies:new Set(),drafts:new Map(),page:'chat',peer:'小林',ready:true,busy:false,follow:false,draft:{value:'你好',readOnly:false,focus(){}},status:{textContent:''},messages:{scrollTop:0,scrollHeight:20},update(){},localDay:()=> '2026-10-30',Date,$:()=>heading,row(role,text){const item={role,text,row:{classList:{add(){}}}};rows.push(item);return item;},invoke:()=>new Promise((ok,no)=>{resolve=ok;reject=no;})};
 vm.createContext(state);vm.runInContext(source.slice(source.indexOf('async function submit()'),source.indexOf("messages.addEventListener('scroll'")),state);
 return{state,rows,heading,resolve:value=>resolve(value),reject:error=>reject(error)};
}
test('chat shows typing in the heading and inserts only the complete reply',async()=>{
 const s=setup(),request=s.state.submit();assert.equal(s.heading.textContent,'对方正在输入...');assert.equal(s.rows.length,1);assert.equal(s.rows[0].role,'user');
 s.resolve('你好呀！\n今天怎么样？');await request;assert.equal(s.rows.length,2);assert.equal(s.rows[1].text,'你好呀！\n今天怎么样？');assert.equal(s.heading.textContent,'小林');assert.equal(s.state.draft.value,'');assert.equal(s.state.busy,false);
});
test('failure clears typing without creating an incomplete reply and retains the draft',async()=>{
 const s=setup(),request=s.state.submit();s.reject(new Error('离线'));await request;
 assert.equal(s.rows.length,1);assert.equal(s.state.draft.value,'你好');assert.equal(s.heading.textContent,'小林');assert.equal(s.state.busy,false);assert.match(s.state.status.textContent,/离线/);
});

test('character may send several complete messages as separate chat entries',async()=>{
 const s=setup(),request=s.state.submit();s.resolve({messages:['先歇一会儿吧。','刚才那件事，我有个不同的想法。'],silent:false});await request;
 assert.deepEqual(s.rows.map(r=>r.text),['你好','先歇一会儿吧。','刚才那件事，我有个不同的想法。']);assert.equal(s.state.busy,false);
});
test('intentional silence finishes successfully without an empty bubble or retry error',async()=>{
 const s=setup(),request=s.state.submit();s.resolve({messages:[],silent:true});await request;
 assert.equal(s.rows.length,1);assert.equal(s.state.draft.value,'');assert.equal(s.state.busy,false);assert.doesNotMatch(s.state.status.textContent,/失败/);
});
test('leaving chat while waiting preserves navigation and completes in the background',async()=>{
 const s=setup(),request=s.state.submit();s.state.page='home';s.heading.textContent='微信';
 s.resolve('晚点聊～');await request;
 assert.equal(s.heading.textContent,'微信');assert.equal(s.rows.length,1);assert.equal(s.state.busy,false);
});
test('switching character never appends the previous character reply to the new chat',async()=>{
 const s=setup(),request=s.state.submit();s.state.peer='小雨';s.state.draft.value='新草稿';s.state.busy=false;s.heading.textContent='小雨';
 s.resolve('这是小林的回复');await request;
 assert.equal(s.rows.length,1);assert.equal(s.heading.textContent,'小雨');assert.equal(s.state.draft.value,'新草稿');
});
test('moment composer is hidden until camera opens it and closing preserves the draft',()=>{
 const elements=Object.fromEntries(['moment-editor','more','moments-panel','moment-text','moment-cancel'].map(id=>['#'+id,{hidden:true,value:id==='moment-text'?'未发完的内容':'',scrollTop:100,focus(){this.focused=true;},setAttribute(k,v){this[k]=v;}}]));
 const state={$:id=>elements[id]};vm.createContext(state);const start=source.indexOf('let momentBusy=');vm.runInContext(source.slice(start,source.indexOf('async function friendUpdate',start)),state);
 state.setMomentEditor(true);assert.equal(elements['#moment-editor'].hidden,false);assert.equal(elements['#more']['aria-expanded'],'true');assert.equal(elements['#moment-text'].focused,true);
 elements['#moment-cancel'].onclick();assert.equal(elements['#moment-editor'].hidden,true);assert.equal(elements['#moment-text'].value,'未发完的内容');assert.equal(elements['#more']['aria-expanded'],'false');
});
test('manual friend updates never resubmit a user post and posting does not force a friend reply',async()=>{
 const elements=Object.fromEntries(['moment-post','moment-refresh','moment-text','moment-status','moment-cancel','moment-editor','more'].map(id=>['#'+id,{value:id==='moment-text'?'今天散步':'',disabled:false,textContent:'',hidden:true,setAttribute(){},focus(){}}]));
 const calls=[];const state={$:id=>elements[id],localDay:()=> '2026-10-30',Date,renderMoments:async()=>{},invoke:async cmd=>{calls.push(cmd);if(cmd==='wechat_friend_post')throw new Error('离线');}};
 vm.createContext(state);const begin=source.indexOf('let momentBusy=');vm.runInContext(source.slice(begin,source.indexOf("document.addEventListener('keydown'",begin)),state);
 await elements['#moment-post'].onclick();
 assert.deepEqual(calls,['wechat_post']);
 assert.match(elements['#moment-status'].textContent,/已发表/);
 await elements['#moment-refresh'].onclick();
 assert.deepEqual(calls,['wechat_post','wechat_friend_post']);
 assert.equal(elements['#moment-text'].value,'');
 assert.match(elements['#moment-status'].textContent,/暂未更新/);
 assert.equal(elements['#moment-post'].disabled,false);
 assert.equal(elements['#moment-refresh'].disabled,false);
});

for(const permanent of [false,true])test(`friend confirmation forwards permanent=${permanent} and exits character settings after deletion`,async()=>{
 const elements=Object.fromEntries(['friend-delete-history','friend-delete-confirm','friend-delete-cancel','friend-delete','friend-delete-status','role-panel','more','home-status'].map(id=>['#'+id,{checked:permanent,disabled:false,hidden:false,close(){this.open=false;},setAttribute(){}}]));
 const calls=[];const state={removingFriend:false,pendingFriend:'小林',people:[{name:'小林',avatar:'glasses'}],deletedFriends:[],drafts:new Map([['小林','草稿']]),peer:'小林',peerAvatar:'glasses',draft:{value:'草稿'},messages:{inert:true,replaceChildren(){}},ready:true,roleData:{},roleDirty:true,update(){},renderHome(){},showPage(){},document:{querySelector(){return{inert:true,focus(){}};}},$:id=>elements[id],invoke:async(cmd,args)=>{calls.push([cmd,args]);if(cmd==='wechat_home')return{people:[],deleted:permanent?[]:[{name:'小林'}]};}};
 vm.createContext(state);const start=source.indexOf("$('#friend-delete-confirm').onclick=");vm.runInContext(source.slice(start,source.indexOf('async function restoreFriend',start)),state);
 await elements['#friend-delete-confirm'].onclick();
 assert.equal(calls[0][0],'wechat_remove_friend');assert.equal(calls[0][1].deleteHistory,permanent);
 assert.equal(state.peer,'');assert.equal(state.drafts.has('小林'),false);assert.equal(elements['#role-panel'].hidden,true);assert.equal(state.messages.inert,false);
 assert.equal(state.deletedFriends.length,permanent?0:1);
 assert.match(elements['#home-status'].textContent,permanent?/无法恢复/:/可在通讯录恢复/);
});

import test from 'node:test';
import assert from 'node:assert/strict';
import { mentionContext, mentionCandidates, replaceMention, parsePluginMentions, selectedMentionRanges } from '../src/plugin-mentions.js';
const plugins = [{enabled:true,manifest:{id:'quick-stats',name:'快速统计',description:'统计数字',tools:[{name:'summarize',description:'计算平均值'}]}},
  {enabled:false,manifest:{id:'disabled',name:'已停用',description:'',tools:[{name:'run',description:''}]}}];

test('picker finds plugin/tool names at the caret, supports Chinese and fullwidth @', () => {
  assert.equal(mentionCandidates(plugins,'快速')[0].fill,'@quick-stats');
  assert.equal(mentionCandidates(plugins,'quick-stats/')[0].fill,'@quick-stats/summarize');
  assert.ok(mentionCandidates(plugins,'').every(c=>!c.fill.includes('disabled')));
  assert.deepEqual(mentionContext('帮我@qui'),{start:2,end:6,query:'qui'});
  assert.equal(mentionContext('＠快速').query,'快速');
  assert.equal(mentionContext('user@example.com'),null);
});
test('completion replaces only the mention and preserves surrounding text and caret', () => {
  assert.deepEqual(replaceMention('请 @qui 计算平均值',6,'@quick-stats'),{text:'请 @quick-stats 计算平均值',caret:15});
  assert.equal(replaceMention('请 @quick-stats 计算',6,'@quick-stats/summarize').text,'请 @quick-stats/summarize 计算');
});
test('natural-language mentions create explicit tool targets', () => {
  const result=parsePluginMentions('@quick-stats 计算 2、4、6 的平均值',plugins);
  assert.deepEqual(result.targets,[{id:'quick-stats'}]);assert.equal(result.direct,null);
  assert.deepEqual(parsePluginMentions('请 @快速统计/summarize 求平均值',plugins).targets,[{id:'quick-stats',tool:'summarize'}]);
  assert.equal(selectedMentionRanges('请 @quick-stats 求平均值',plugins)[0].name,'quick-stats');
});
test('JSON mentions remain direct/offline and preserve literal @ inside string arguments', () => {
  const result=parsePluginMentions('@quick-stats {"values":[2,4,6]}',plugins);
  assert.deepEqual(result.direct,{action:'run',id:'quick-stats',tool:'summarize',args:{values:[2,4,6]}});
  const literal=parsePluginMentions('@quick-stats {"text":"hello @not-a-plugin"}',plugins);
  assert.equal(literal.direct.args.text,'hello @not-a-plugin');
});
test('emails, code and quoted examples do not select plugins', () => {
  for(const text of ['邮箱 user@example.com','示例 `@missing`','```text\n@missing\n```','他说“用 @missing 查询”','"hello @missing"']) {
    assert.deepEqual(parsePluginMentions(text,plugins).targets,[]);
  }
});
test('invalid, ambiguous, disabled or unfinished mentions never silently go to AI', () => {
  for(const text of ['@','@missing 查询','@disabled 查询','@quick-stats/missing 查询','@quick-stats','@quick-stats {bad}']) {
    assert.throws(()=>parsePluginMentions(text,plugins),text);
  }
  const duplicate={enabled:true,manifest:{...plugins[0].manifest,id:'another'}};
  assert.throws(()=>parsePluginMentions('@快速统计 查询',[...plugins,duplicate]));
});

const builtins=[
  {builtin:true,enabled:true,default_tool:'add',empty_tool:'show',manifest:{id:'note',name:'便签',aliases:['notes','便签'],description:'管理便签',tools:[
    {name:'add',description:'添加便签',parameters:{required:['text']}}, {name:'show',description:'打开便签',parameters:{}}, {name:'list',description:'查看便签',parameters:{}},
  ]}},
  {builtin:true,enabled:true,default_tool:'open',empty_tool:'open',manifest:{id:'clock',name:'计时器',aliases:['计时器'],description:'打开计时设置',tools:[
    {name:'open',description:'设置计时器',parameters:{required:['mode','seconds','restart']}}, {name:'status',description:'查看状态',parameters:{}},
  ]}},
];
test('built-in notes and clock support aliases, natural language and bare opening',()=>{
  for(const [name,id] of [['note','note'],['notes','note'],['便签','note'],['clock','clock'],['计时器','clock']]){
    assert.deepEqual(parsePluginMentions(`@${name} 帮我处理`,builtins).targets,[{id,builtin:true}]);
  }
  assert.deepEqual(parsePluginMentions('@note',builtins).direct,{action:'builtin',id:'note',tool:'show',args:{}});
  assert.deepEqual(parsePluginMentions('@clock ',builtins).direct,{action:'builtin',id:'clock',tool:'open',args:{}});
  assert.deepEqual(parsePluginMentions('@note {"text":"买牛奶"}',builtins).direct,{action:'builtin',id:'note',tool:'add',args:{text:'买牛奶'}});
  assert.throws(()=>parsePluginMentions('@note/add',builtins));
  assert.deepEqual(mentionCandidates(builtins,'').slice(0,2).map(t=>t.fill),['@note','@clock']);
});
test('same-named third-party plugins have an explicit namespace and cannot shadow built-ins',()=>{
  const thirdParty={enabled:true,manifest:{id:'note',name:'第三方便签',description:'测试同名插件',tools:[{name:'list',description:'列出',parameters:{}}]}};
  const all=[...builtins,thirdParty];
  assert.ok(mentionCandidates(all,'').some(t=>t.fill==='@plugin/note'));
  assert.deepEqual(parsePluginMentions('@note @plugin/note 查询',all).targets,[{id:'note',builtin:true},{id:'note'}]);
  assert.deepEqual(parsePluginMentions('@plugin/note/list 查询',all).targets,[{id:'note',tool:'list'}]);
  const aliasCollision={...thirdParty,manifest:{...thirdParty.manifest,id:'notes'}};
  assert.deepEqual(parsePluginMentions('@notes 查询',[...builtins,aliasCollision]).targets,[{id:'note',builtin:true}]);
});

test('removing a tool retains the request and other tools while adjusting the caret',async()=>{
 const {removeMention}=await import('../src/plugin-mentions.js');
 const text='@note @clock 保留这段内容';
 const result=removeMention(text,{start:0,end:5},text.length);
 assert.equal(result.text,'@clock 保留这段内容');assert.equal(result.start,result.text.length);
 const inside=removeMention(text,{start:0,end:5},3);assert.equal(inside.start,0);
 assert.throws(()=>removeMention(text,{start:8,end:100},0));
});

test('dismiss space keeps the caret and plain request aligned and is stable on repeat',async()=>{
 const {spaceMentionControls,removeMention}=await import('../src/plugin-mentions.js');
 const value='@note 帮我记下来',range={start:0,end:5};
 const spaced=spaceMentionControls(value,[range],value.length);
 assert.equal(spaced.text,'@note'+' '.repeat(11)+'帮我记下来');assert.equal(spaced.start,spaced.text.length);
 assert.equal(spaceMentionControls(spaced.text,[range],spaced.start).text,spaced.text);
 assert.equal(removeMention(spaced.text,range,spaced.start).text,'帮我记下来');
});

test('multiple tool chips keep independent gaps and the caret clears the final chip',async()=>{
 const {spaceMentionControls,removeMention}=await import('../src/plugin-mentions.js');
 const value='@note @clock ',ranges=[{start:0,end:5},{start:6,end:12}];
 const result=spaceMentionControls(value,ranges,value.length);
 assert.equal(result.text,'@note'+' '.repeat(11)+'@clock'+' '.repeat(11));
 assert.equal(result.start,result.text.length);
 // The painted chip reserves seven spaces for its close button; four more
 // remain outside its background for the next chip and the editable caret.
 assert.equal(result.text.length-('@note'.length+11+'@clock'.length+7),4);
 const clockStart=result.text.indexOf('@clock');
 assert.equal(removeMention(result.text,{start:clockStart,end:clockStart+6},result.start).text,'@note'+' '.repeat(11));
 assert.equal(spaceMentionControls(result.text,[{start:0,end:5},{start:clockStart,end:clockStart+6}],result.start).text,result.text);
});

test('WeChat opens directly with a required peer name and keeps JSON direct calls',()=>{
 const plugins=[{builtin:true,enabled:true,default_tool:'open',empty_tool:'open',manifest:{id:'wechat',name:'WeChat',aliases:['微信'],tools:[{name:'open',parameters:{type:'object',required:['peer_name']}}]}}];
 assert.deepEqual(parsePluginMentions('@wechat 小林',plugins).direct,{action:'builtin',id:'wechat',tool:'open',args:{peer_name:'小林'}});
 assert.equal(parsePluginMentions('@微信 张三',plugins).direct.args.peer_name,'张三');
 assert.equal(parsePluginMentions('@wechat {"peer_name":"小林"}',plugins).direct.args.peer_name,'小林');
 assert.throws(()=>parsePluginMentions('@wechat',plugins));
});

test('broadcast supports bare opening, Chinese alias and offline preview arguments',()=>{
 const plugins=[{builtin:true,enabled:true,default_tool:'prepare',empty_tool:'open',manifest:{id:'broadcast',name:'广播站',aliases:['广播'],tools:[{name:'open',parameters:{required:[]}},{name:'prepare',parameters:{required:['title','content']}}]}}];
 assert.deepEqual(parsePluginMentions('@广播',plugins).direct,{action:'builtin',id:'broadcast',tool:'open',args:{}});
 assert.deepEqual(parsePluginMentions('@broadcast {"title":"通知","content":"三点见"}',plugins).direct,{action:'builtin',id:'broadcast',tool:'prepare',args:{title:'通知',content:'三点见'}});
 assert.equal(parsePluginMentions('@broadcast 帮我广播明天开会',plugins).targets[0].id,'broadcast');
});

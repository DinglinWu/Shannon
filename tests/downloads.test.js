import test from 'node:test';
import assert from 'node:assert/strict';
import {taskView,bytes} from '../src/downloads-model.js';
import {parsePluginMentions} from '../src/plugin-mentions.js';
test('download progress handles unknown size, completion and resume states',()=>{
 assert.equal(taskView({status:'running',size:100,progress:{downloaded:50,speed:10}}).percent,50);
 assert.match(taskView({status:'running',size:100,progress:{downloaded:50,speed:10}}).detail,/剩余约 5 秒/);
 assert.equal(taskView({status:'running',size:0,progress:{downloaded:50}}).percent,0);
 assert.equal(taskView({status:'done'}).percent,100);
 assert.equal(taskView({status:'pause'}).canContinue,true);
 assert.equal(taskView({status:'done'}).canContinue,false);
 assert.equal(bytes(1024),'1.0 KiB');
});
test('download mentions prepare links offline and leave natural language to selected AI tools',()=>{
 const plugins=[{enabled:true,builtin:true,empty_tool:'open',default_tool:'prepare',manifest:{id:'download',name:'下载',aliases:['下载器'],tools:['open','prepare','list','control'].map(name=>({name,parameters:{required:name==='prepare'?['url']:[]}}))}}];
 assert.equal(parsePluginMentions('@下载',plugins).direct.tool,'open');
 assert.deepEqual(parsePluginMentions('@下载 https://example.com/a.zip',plugins).direct.args,{url:'https://example.com/a.zip'});
 assert.equal(parsePluginMentions('@download 帮我暂停第一个下载',plugins).direct,null);
});

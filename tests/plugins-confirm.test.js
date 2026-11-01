import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
const source=(await readFile(new URL('../src/plugins-confirm.js',import.meta.url),'utf8')).replace(/^import .*;\r?$/gm,'');
async function render(readme) {
  const nodes=new Map();
  const get=key=>{if(!nodes.has(key))nodes.set(key,{textContent:'',disabled:false,children:[],append(child){this.children.push(child);},set innerHTML(_){throw new Error('README must never enter HTML');}});return nodes.get(key);};
  const request={action:'install',source:'fixture.zip',sha256:'abc',readme,manifest:{id:'sample',name:'示例',version:'1.0.0',author:'开发者',description:'功能',permissions:[],tools:[]}};
  vm.runInNewContext(source,{invoke:async()=>request,document:{querySelector:get,createElement:()=>({textContent:''}),addEventListener(){}}});
  await new Promise(resolve=>setImmediate(resolve));return get;
}
test('installation displays the exact README as inert text',async()=>{
  const raw='---\nauthor: 开发者\nversion: 1.0.0\n---\n<img src=x onerror=alert(1)>\n功能说明';
  const get=await render(raw);assert.equal(get('#readme').textContent,raw);assert.equal(get('#approve').disabled,false);
});
test('legacy installed packages show an explicit missing README message',async()=>{
  const get=await render('');assert.match(get('#readme').textContent,/旧版安装/);
});

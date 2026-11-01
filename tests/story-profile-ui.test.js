import {test} from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
const source=readFileSync(new URL('../src/story-profile-ui.js',import.meta.url),'utf8').replaceAll('export function','function');
function render(profile){const make=tag=>({tag,children:[],append(...values){this.children.push(...values);},setAttribute(k,v){this[k]=v;}});const ctx={document:{createElement:make}};vm.createContext(ctx);vm.runInContext(source,ctx);const root=ctx.renderCharacterProfile(profile);const all=[];function visit(n){all.push(n);for(const c of n.children||[])visit(c);}visit(root);return all;}
test('legacy characters are not assigned invented neutral values',()=>{const nodes=render(null);assert.equal(nodes.filter(n=>n.tag==='meter').length,0);assert.ok(nodes.some(n=>n.textContent?.includes('未知维度保持未定')));});
test('known numeric traits and unknown dimensions remain distinct',()=>{const nodes=render({traits:{direct:70},speech:['short','dry_humor'],anchors:{home:'海边长大'},relations:{小雨:{trust:65}},mood:{tension:20}});assert.equal(nodes.filter(n=>n.tag==='meter').length,1);assert.equal(nodes.find(n=>n.tag==='meter').value,70);assert.equal(nodes.filter(n=>n.textContent==='未定').length,5);assert.ok(nodes.some(n=>n.textContent==='说话习惯：短句、冷幽默'));assert.ok(nodes.some(n=>n.textContent==='海边长大'));assert.ok(nodes.some(n=>n.textContent==='小雨：信任 65'));});

test('profile history describes numeric changes without repeating unrelated traits',()=>{const ctx={};vm.createContext(ctx);vm.runInContext(source,ctx);const changes=ctx.describeProfileChanges({traits:{direct:70,social:30},speech:['short']},{traits:{direct:70,social:33},speech:['short','dry_humor']});assert.equal(changes.length,2);assert.equal(changes[0],'社交倾向：30 → 33');assert.equal(changes[1],'习惯增加：冷幽默');});

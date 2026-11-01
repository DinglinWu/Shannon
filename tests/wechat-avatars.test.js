import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {catalog,avatarId,avatarPath,isCustomAvatar} from '../src/wechat-avatars.js';
test('every selectable avatar has a unique bundled asset and cannot load arbitrary paths',()=>{
 assert.equal(catalog.length,16);assert.equal(new Set(catalog.map(a=>a.id)).size,16);
 for(const item of catalog){const svg=readFileSync(new URL(`../public/avatars/${item.id}.svg`,import.meta.url),'utf8');assert.match(svg,/<svg /);assert.doesNotMatch(svg,/<script|<foreignObject|href=/i);}
 assert.equal(avatarId('sea','小林'),'sea');assert.equal(avatarId('', '小林'),avatarId('', '小林'));
 assert.match(avatarPath('https://example.com/a.png','小林'),/^\/avatars\/[a-z-]+\.svg$/);
 assert.match(avatarPath('../../secret','小林'),/^\/avatars\/[a-z-]+\.svg$/);
});

test('uploaded avatar identifiers are preserved while paths stay bundled until resolved locally',()=>{
 const id='custom-'+'a'.repeat(64);assert.equal(avatarId(id,'小林'),id);assert.equal(isCustomAvatar(id),true);
 for(const invalid of ['custom-../../file','custom-'+'A'.repeat(64),'data:image/svg+xml,<svg/>','https://example.com/a'])assert.equal(isCustomAvatar(invalid),false);
 assert.match(avatarPath(id,'小林'),/^\/avatars\/[a-z-]+\.svg$/);
});

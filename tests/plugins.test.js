import test from 'node:test';
import assert from 'node:assert/strict';
import { parsePluginCommand, pluginValue, formatPluginList, formatBuiltinResult } from '../src/plugins.js';
import { renderMarkdown } from '../src/markdown.js';
import { sandboxDocument } from '../src/plugins-runtime.js';

test('plugin commands preserve full paths and validate JSON arguments', () => {
  assert.equal(parsePluginCommand('/clock'), null);
  assert.deepEqual(parsePluginCommand('/plugin manage'),{action:'manage'});
  assert.deepEqual(parsePluginCommand('/plugin list'),{action:'list'});
  assert.deepEqual(parsePluginCommand('/插件'), {action:'manage'});
  assert.deepEqual(parsePluginCommand('/plugin install "E:\\My Plugins\\stats.zip"'), {action:'install',source:'E:\\My Plugins\\stats.zip'});
  assert.deepEqual(parsePluginCommand('/plugin install https://example.com/stats.zip'), {action:'install',source:'https://example.com/stats.zip'});
  assert.deepEqual(parsePluginCommand('/plugin run stats/sum {"values":[1,2]}'), {action:'run',id:'stats',tool:'sum',args:{values:[1,2]}});
  assert.deepEqual(parsePluginCommand('/stats/sum {"values":[1,2]}'), {action:'run',id:'stats',tool:'sum',args:{values:[1,2]}});
  for (const input of ['/plugin run x/y []','/plugin run x/y null','/plugin run x/y {bad}','/plugin enable ../x','/plugin install "oops']) {
    assert.throws(() => parsePluginCommand(input));
  }
});
test('plugin list labels cannot inject links, images or raw HTML', () => {
  const text = formatPluginList({plugins:[{enabled:true,manifest:{id:'safe',name:'[click](javascript:alert(1))',version:'1.0.0',description:'![image](https://example.com) <script>bad</script>'}}]});
  const html = renderMarkdown(text);
  assert.doesNotMatch(html, /<a |<img |<script>/);
  assert.match(formatPluginList({plugins:[]}), /尚未安装插件/);
});
test('untrusted plugin metadata cannot close the JSON output fence', () => {
  const value = pluginValue({name:'~~~\n<script>bad</script>\n~~~~'});
  assert.ok(value.startsWith('~~~~~json\n'));
  assert.ok(value.endsWith('\n~~~~~'));
});
test('built-in feedback distinguishes opening settings from starting and preserves save failures',()=>{
  assert.match(formatBuiltinResult('clock','open',{already_active:false}),/确认后开始/);
  assert.match(formatBuiltinResult('clock','open',{already_active:true}),/已有计时器/);
  assert.match(formatBuiltinResult('note','add',{display_error:'failed'}),/内容已保存/);
});
test('sandbox bootstrap keeps untrusted code inside a worker string and blocks external resources', () => {
  const html = sandboxDocument('</script><script>parent.bad=true</script>', 'test-nonce');
  assert.equal((html.match(/<script /g)||[]).length, 1);
  assert.equal((html.match(/<\/script>/g)||[]).length, 1);
  assert.match(html, /connect-src 'none'/);
  assert.match(html, /worker-src blob:/);
  assert.doesNotMatch(html, /allow-same-origin|unsafe-eval|unsafe-inline/);
  assert.doesNotMatch(html, /<script>parent/);
});

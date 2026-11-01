import test from 'node:test';
import assert from 'node:assert/strict';
import { parseComponentCommand, formatComponents, componentProgress } from '../src/components.js';

test('component commands accept only fixed component names and exact actions', () => {
  assert.deepEqual(parseComponentCommand('/component install voice'), {action:'install',name:'voice'});
  assert.deepEqual(parseComponentCommand('/gesture install'), {action:'install',name:'gesture'});
  assert.deepEqual(parseComponentCommand('/语音 安装'), {action:'install',name:'voice'});
  assert.deepEqual(parseComponentCommand('/components'), {action:'status'});
  assert.equal(parseComponentCommand('解释 /component install voice'), null);
  for (const command of ['/component install shell','/component install voice extra','/component status x']) assert.throws(()=>parseComponentCommand(command));
});
test('component output distinguishes downloading, ready, and failures', () => {
  assert.match(componentProgress({name:'voice',phase:'downloading',received:5,total:10}), /50%/);
  assert.match(componentProgress({name:'gesture',phase:'installed'}), /gesture start/);
  assert.match(componentProgress({name:'voice',phase:'error',error:'校验失败'}), /校验失败/);
  assert.match(formatComponents({components:[{name:'voice',installed:false},{name:'gesture',installed:true}]}), /未安装[\s\S]*已安装/);
});

import test from 'node:test';
import assert from 'node:assert/strict';
import {generateKeyPairSync, sign, createHash, randomBytes} from 'node:crypto';
import {verifyArtifact} from '../scripts/build/verify_update_signature.mjs';
function fixture(){
  const {publicKey,privateKey}=generateKeyPairSync('ed25519');
  const key=publicKey.export({format:'der',type:'spki'}).subarray(-32);
  const id=randomBytes(8), data=Buffer.from('update fixture'), comment='timestamp:123\tfile:setup.exe';
  const signature=sign(null,createHash('blake2b512').update(data).digest(),privateKey);
  const packet=Buffer.concat([Buffer.from('ED'),id,signature]);
  const global=sign(null,Buffer.concat([signature,Buffer.from(comment)]),privateKey);
  const pub=Buffer.from('untrusted comment: fixture\n'+Buffer.concat([Buffer.from('Ed'),id,key]).toString('base64')+'\n').toString('base64');
  const sig=Buffer.from('untrusted comment: fixture\n'+packet.toString('base64')+'\ntrusted comment: '+comment+'\n'+global.toString('base64')+'\n').toString('base64');
  return {data,pub,sig};
}
test('updater release signature must match the configured public key and exact artifact',()=>{
  const {data,pub,sig}=fixture();assert.equal(verifyArtifact(data,sig,pub),true);
  assert.throws(()=>verifyArtifact(Buffer.from('changed installer'),sig,pub));
  assert.throws(()=>verifyArtifact(data,sig,fixture().pub));
});
test('trusted comment tampering and invalid signature encodings are rejected',()=>{
  const {data,pub,sig}=fixture();
  const tampered=Buffer.from(Buffer.from(sig,'base64').toString().replace('timestamp:123','timestamp:124')).toString('base64');
  assert.throws(()=>verifyArtifact(data,tampered,pub));
  assert.throws(()=>verifyArtifact(data,'invalid',pub));
});

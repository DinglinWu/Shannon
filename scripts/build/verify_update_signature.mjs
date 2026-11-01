import {createHash, createPublicKey, verify} from 'node:crypto';
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';

// Tauri stores the complete Minisign public key and detached signature as base64.
// Verify both the artifact signature and the authenticated trusted comment.
export function verifyArtifact(data, encodedSignature, encodedPublicKey) {
  const keyText=Buffer.from(encodedPublicKey.trim(),'base64');
  const key=keyText.length===42?keyText:Buffer.from(keyText.toString('utf8').trim().split(/\r?\n/)[1]||'','base64');
  const lines=Buffer.from(encodedSignature.trim(),'base64').toString('utf8').trim().split(/\r?\n/);
  const packet=Buffer.from(lines[1]||'','base64');
  const global=Buffer.from(lines[3]||'','base64');
  if(key.length!==42||!['Ed','ED'].includes(key.subarray(0,2).toString())||packet.length!==74||global.length!==64||!lines[2]?.startsWith('trusted comment: '))throw new Error('更新公钥或签名格式无效');
  if(!key.subarray(2,10).equals(packet.subarray(2,10)))throw new Error('签名私钥与应用内公钥不匹配');
  const algorithm=packet.subarray(0,2).toString();
  if(!['Ed','ED'].includes(algorithm))throw new Error('不支持的更新签名算法');
  const publicKey=createPublicKey({key:Buffer.concat([Buffer.from('302a300506032b6570032100','hex'),key.subarray(10)]),format:'der',type:'spki'});
  const signature=packet.subarray(10);
  const payload=algorithm==='ED'?createHash('blake2b512').update(data).digest():data;
  if(!verify(null,payload,publicKey,signature)||!verify(null,Buffer.concat([signature,Buffer.from(lines[2].slice(17))]),publicKey,global))throw new Error('更新签名验证失败');
  return true;
}
if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href){
  try{
    const [artifact,signature,configuration]=process.argv.slice(2);
    const config=JSON.parse(readFileSync(configuration,'utf8'));
    verifyArtifact(readFileSync(artifact),readFileSync(signature,'utf8'),config.plugins.updater.pubkey);
    console.log('PASS: updater signature matches the application public key.');
  }catch(error){console.error(error.message);process.exitCode=1;}
}

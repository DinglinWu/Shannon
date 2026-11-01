"""Exercise the real private pipe in a debug runtime without opening devices or plugin windows."""
import json, os, queue, subprocess, threading, zipfile, hashlib
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def main():
    catalog=json.loads((ROOT/'src-tauri/official-plugins.json').read_text(encoding='utf-8'))
    runtime_spec=next(p['runtime'] for p in catalog['plugins'] if p['id']=='clock')
    if runtime_spec.get('profile')!='debug':raise RuntimeError('Smoke isolation requires packages built with --debug; release builds never honor test data roots.')
    testroot=ROOT/'release/plugin-runtime-smoke';testroot.mkdir(parents=True,exist_ok=True)
    (testroot/'builtin-plugins.json').write_text(json.dumps({'clock':'enabled','note':'removed','story':'removed','voice':'removed','gesture':'removed','wechat':'removed'}),encoding='utf-8')
    catalog=json.loads((ROOT/'src-tauri/official-plugins.json').read_text(encoding='utf-8'))
    archive=ROOT/'release/plugins'/next(p['runtime'] for p in catalog['plugins'] if p['id']=='clock')['url'].rsplit('/',1)[1]
    raw=archive.read_bytes();assert hashlib.sha256(raw).hexdigest()==next(p['runtime'] for p in catalog['plugins'] if p['id']=='clock')['sha256']
    with zipfile.ZipFile(archive) as z: runtime=z.read('runtime.exe')
    assert hashlib.sha256(runtime).hexdigest()==next(p['runtime'] for p in catalog['plugins'] if p['id']=='clock')['executable_sha256']
    executable=testroot/'runtime.exe';executable.write_bytes(runtime)
    env=os.environ.copy();env['SHANNON_PLUGIN_TEST_ROOT']=str(testroot)
    p=subprocess.Popen([str(executable),'--native-service','--plugin-id','clock'],cwd=ROOT,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding='utf-8',creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
    q=queue.Queue()
    def read():
        for line in p.stdout:q.put(json.loads(line))
    threading.Thread(target=read,daemon=True).start()
    try:
        first=q.get(timeout=30);assert first.get('ready') and first.get('plugin_id')=='clock' and first.get('protocol')==1,first
        def call(i,kind,name,args):
            p.stdin.write(json.dumps({'id':i,'kind':kind,'name':name,'args':args})+'\n');p.stdin.flush()
            while True:
                response=q.get(timeout=15)
                if response.get('id')==i:return response
        r=call(1,'command','clock_active',{});assert r.get('ok') and r.get('data') is False,r
        r=call(2,'tool','notes_list',{});assert not r.get('ok'),r
        r=call(3,'command','run_shell',{'cmd':'anything'});assert not r.get('ok'),r
        print('PASS: real runtime handshake, fixed command round trip, disabled plugin rejection and arbitrary-command rejection.')
    finally:
        p.stdin.close()
        try:p.wait(timeout=10)
        except subprocess.TimeoutExpired:p.kill();p.wait()
if __name__=='__main__':main()

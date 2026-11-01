"""Exercise numeric profile persistence through the packaged native tool entry points."""
import hashlib,json,os,queue,subprocess,threading,zipfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def main():
    root=ROOT/'release/story-profile-smoke';root.mkdir(parents=True,exist_ok=True)
    catalog=json.loads((ROOT/'src-tauri/official-plugins.json').read_text(encoding='utf-8'));spec=catalog['runtime']
    package=ROOT/'release/plugins'/spec['url'].rsplit('/',1)[1]
    raw=package.read_bytes();assert hashlib.sha256(raw).hexdigest()==spec['sha256']
    with zipfile.ZipFile(package) as z:exe=z.read('runtime.exe')
    assert hashlib.sha256(exe).hexdigest()==spec['executable_sha256']
    executable=root/'runtime.exe';executable.write_bytes(exe)
    (root/'builtin-plugins.json').write_text(json.dumps({'story':'enabled'}),encoding='utf-8')
    character={'name':'小林','personality':'严谨但会讲冷笑话','profile':{'traits':{'direct':70,'social':30},'speech':['short','dry_humor'],'anchors':{'home':'海边长大'},'relations':{'我':{'trust':60}},'mood':{'tension':20}}}
    story={'id':1,'revision':1,'draft':{'title':'隔离测试','world':'现代','player':'我','scene':'车站','characters':[character]},'turns':[],'tools':[]}
    (root/'stories.json').write_text(json.dumps({'active':1,'stories':[story]},ensure_ascii=False),encoding='utf-8')
    env=os.environ.copy();env['SHANNON_PLUGIN_TEST_ROOT']=str(root)
    p=subprocess.Popen([str(executable),'--native-service'],env=env,cwd=ROOT,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding='utf-8',creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0));q=queue.Queue()
    def reader():
        for line in p.stdout:q.put(json.loads(line))
    threading.Thread(target=reader,daemon=True).start()
    try:
        assert q.get(timeout=30).get('ready')
        def call(i,name,args):
            p.stdin.write(json.dumps({'id':i,'kind':'tool','name':name,'args':args})+'\n');p.stdin.flush()
            while True:
                r=q.get(timeout=15)
                if r.get('id')==i:assert r.get('ok'),r;return r['data']
        data=call(1,'story_read',{'id':1});assert data['character_profiles'][0]['profile']['traits']['direct']==70
        draft=data['story']['draft'];assert 'profile' not in draft['characters'][0]
        draft['characters'][0]['scene_goal']='看书'
        call(2,'story_update',{'id':1,'revision':1,'draft':draft})
        data=call(3,'story_read',{'id':1});assert data['character_profiles'][0]['profile']['traits']['direct']==70
        saved=json.loads((root/'stories.json').read_text(encoding='utf-8'));assert saved['stories'][0]['draft']['characters'][0]['profile']['anchors']['home']=='海边长大'
        print('PASS: packaged runtime reads compact numeric personas, preserves them across tool-based settings edits, and retains original references.')
    finally:
        p.stdin.close()
        try:p.wait(timeout=10)
        except subprocess.TimeoutExpired:p.kill();p.wait()
if __name__=='__main__':main()

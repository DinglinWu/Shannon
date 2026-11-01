"""Real Gopeed 1.9.3 contract test; only local test files/network, no user data.
Run: python tests/gopeed-smoke.py (requires pinned release/components ZIP).
"""
import hashlib, http.server, json, pathlib, socket, subprocess, threading, time, urllib.request, zipfile, uuid
ROOT=pathlib.Path(__file__).resolve().parents[1]
ZIP=ROOT/'release/components/gopeed-web-v1.9.3-windows-amd64.zip'
assert hashlib.sha256(ZIP.read_bytes()).hexdigest()=='ee3d41fa19c366b820a2483619328156579108ac03c53ea35cdb33327320f756'
WORK=ROOT/'release'/('gopeed-smoke-'+uuid.uuid4().hex[:8]);WORK.mkdir()
DATA=bytes(range(256))*32768
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  a,b=0,len(DATA)-1
  h=self.headers.get('Range')
  if h:
   ran=h.removeprefix('bytes=').split('-');a=int(ran[0]);b=int(ran[1]) if ran[1] else b
  self.send_response(206 if h else 200)
  self.send_header('Accept-Ranges','bytes');self.send_header('Content-Length',str(b-a+1))
  self.send_header('Last-Modified','Wed, 21 Oct 2015 07:28:00 GMT')
  if h:self.send_header('Content-Range',f'bytes {a}-{b}/{len(DATA)}')
  self.end_headers()
  if self.command=='HEAD':return
  try:
   for off in range(a,b+1,16384):self.wfile.write(DATA[off:min(b+1,off+16384)]);time.sleep(.02)
  except (ConnectionError,OSError):pass
 def do_HEAD(self):self.do_GET()
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
threading.Thread(target=server.serve_forever,daemon=True).start()
with zipfile.ZipFile(ZIP) as z:(WORK/'gopeed.exe').write_bytes(z.read('gopeed-web-v1.9.3-windows-amd64/gopeed.exe'))
with socket.socket() as so:so.bind(('127.0.0.1',0));port=so.getsockname()[1]
token=uuid.uuid4().hex
cfg={'address':'127.0.0.1','port':port,'apiToken':token,'username':'test','password':uuid.uuid4().hex,'storageDir':str(WORK/'storage'),'whiteDownloadDirs':[str(WORK/'files')],'downloadConfig':{'downloadDir':str(WORK/'files'),'maxRunning':3,'script':{'enable':False},'autoTorrent':{'enable':False},'archive':{'autoExtract':False}}}
(WORK/'files').mkdir();(WORK/'runtime.json').write_text(json.dumps(cfg),encoding='utf-8')
def call(path,method='GET',body=None,auth=True):
 req=urllib.request.Request(f'http://127.0.0.1:{port}/api/v1/{path}',data=json.dumps(body).encode() if body is not None else None,method=method,headers={'Content-Type':'application/json',**({'X-Api-Token':token} if auth else {})})
 with urllib.request.urlopen(req,timeout=10) as r:d=json.load(r)
 assert d['code']==0,d
 return d.get('data')
def start():
 p=subprocess.Popen([str(WORK/'gopeed.exe'),'-c',str(WORK/'runtime.json')],cwd=WORK,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
 for _ in range(60):
  try:call('info');return p
  except Exception:time.sleep(.1)
 p.kill();raise AssertionError('engine failed to start')
proc=None
try:
 proc=start();print('engine startup OK',flush=True)
 try:call('tasks',auth=False);raise AssertionError('unauthenticated access accepted')
 except urllib.error.HTTPError as e:assert e.code==401
 url=f'http://127.0.0.1:{server.server_port}/sample.bin'
 task=call('tasks','POST',{'req':{'url':url},'opts':{'path':str(WORK/'files'),'extra':{'connections':4,'autoTorrent':False,'autoExtract':False}}})
 print('created',task,flush=True);time.sleep(1.5)
 call(f'tasks/{task}/pause','PUT');s=call(f'tasks/{task}');assert s['status']=='pause',s
 print('pause + progress OK',s['progress']['downloaded'],flush=True)
 call('tasks/pause','PUT');proc.kill();proc.wait();proc=start()
 assert call(f'tasks/{task}')['status']=='pause'
 call(f'tasks/{task}/continue','PUT')
 for _ in range(150):
  s=call(f'tasks/{task}')
  if s['status']=='done':break
  assert s['status']!='error',s
  time.sleep(.2)
 assert s['status']=='done',s
 output=WORK/'files/sample.bin';assert output.read_bytes()==DATA
 print('restart + resume + byte integrity OK',flush=True)
 call(f'tasks/{task}?force=false','DELETE');assert output.exists();assert not call('tasks')
 print('cancel preserves actual file OK',flush=True)
finally:
 if proc:proc.kill();proc.wait()
 server.shutdown()
print('PASS',WORK,flush=True)

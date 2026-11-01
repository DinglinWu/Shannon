"""Release boundary checks: no plugin implementation is shipped in the core UI."""
import json, hashlib, zipfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def check():
    core=ROOT/'dist-core'
    assert {p.name for p in core.glob('*.html')}=={'index.html','fx.html','plugins.html','plugin-manager.html'}, 'plugin pages leaked into core bundle'
    for p in (core/'assets').glob('*.js'):
        code=p.read_text(encoding='utf-8')
        assert 'voice-listening' not in code and 'voice-ripples' not in code, f'voice implementation leaked: {p}'
    cargo=(ROOT/'src-tauri/Cargo.toml').read_text(encoding='utf-8');assert 'default = []' in cargo
    catalog=json.loads((ROOT/'src-tauri/official-plugins.json').read_text(encoding='utf-8'))
    assert len(catalog['plugins'])==8
    for p in catalog['plugins']: assert p['author'] and p['description'] and p['features'] and p['source'].startswith('https://gitee.com/')
    for plugin in catalog['plugins']:
        spec=plugin['runtime'];assert spec['id']==plugin['id'] and spec['protocol']==1
        archive=ROOT/'release/plugins'/spec['url'].rsplit('/',1)[1]
        raw=archive.read_bytes();assert len(raw)==spec['bytes'] and hashlib.sha256(raw).hexdigest()==spec['sha256']
        with zipfile.ZipFile(archive) as z:
            expected={'runtime.exe','official-plugin.json','README.md'} | ({'voice-ui.js'} if plugin['id']=='voice' else set())
            assert set(z.namelist())==expected
            assert z.read('README.md').decode('utf-8')==spec['readme']
            assert hashlib.sha256(z.read('README.md')).hexdigest()==spec['readme_sha256']
            identity=json.loads(z.read('official-plugin.json'));assert identity=={'id':plugin['id'],'version':spec['version'],'protocol':1}
            assert hashlib.sha256(z.read('runtime.exe')).hexdigest()==spec['executable_sha256']
            if plugin['id']=='voice':assert hashlib.sha256(z.read('voice-ui.js')).hexdigest()==spec['voice_ui_sha256']
    print('PASS: core assets exclude plugin pages and voice code; separate Gitee package and all metadata match catalog.')
if __name__=='__main__':check()

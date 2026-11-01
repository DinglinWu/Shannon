import importlib.util
import json
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'scripts/build'))
spec=importlib.util.spec_from_file_location('official_build',Path(__file__).resolve().parents[1]/'scripts/build/build_official_plugins.py')
build=importlib.util.module_from_spec(spec);spec.loader.exec_module(build)

class OfficialBuildTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name)
        (self.root/'src-tauri').mkdir();(self.root/'src').mkdir()
        (self.root/'src-tauri/tauri.conf.json').write_text('{"bundle":{"windows":{}}}')
        (self.root/'package.json').write_text('{"version":"1.0.0"}')
        self.catalog=self.root/'src-tauri/official-plugins.json'
        self.original=json.dumps({'runtime':{'sha256':'old'},'plugins':[{'id':i,'name':i,'author':'Test','version':'1.0.0','description':'Feature'} for i in ['note','clock','wechat','story','broadcast','download','voice','gesture']]}).encode()
        self.catalog.write_bytes(self.original)
        for m in json.loads(self.original)['plugins']:
            folder=self.root/'plugins/official'/m['id'];folder.mkdir(parents=True)
            text='---\n'+'\n'.join(f'{key}: {m[key]}' for key in ['id','name','author','version','description'])+'\n---\n# Feature\nFeature details'
            (folder/'README.md').write_text(text,encoding='utf-8')
        (self.root/'src/voice-input.js').write_text('export function initVoiceInput(){}')
        exe=self.root/'src-tauri/target/release/shannon-plugin-runtime.exe';exe.parent.mkdir(parents=True);exe.write_bytes(b'test fixture executable')

    def test_compile_failure_preserves_catalog_and_schemas(self):
        schemas=self.root/'src-tauri/official-tools.json';schemas.write_bytes(b'old tools')
        with patch.object(build,'ROOT',self.root),patch.object(build.subprocess,'run',side_effect=RuntimeError('compile failed')):
            with self.assertRaises(RuntimeError):build.build()
        self.assertEqual(self.catalog.read_bytes(),self.original)
        self.assertEqual(schemas.read_bytes(),b'old tools')

    def test_missing_or_mismatched_readme_preserves_catalog(self):
        readme=self.root/'plugins/official/note/README.md'
        for value in [None,'---\nid: wrong\n---\nBody']:
            if value is None:readme.unlink()
            else:readme.write_text(value,encoding='utf-8')
            with patch.object(build,'ROOT',self.root),patch.object(build.subprocess,'run'),patch.object(build.subprocess,'check_output',return_value=b'[]'):
                with self.assertRaises((ValueError,FileNotFoundError)):build.build()
            self.assertEqual(self.catalog.read_bytes(),self.original)

    def test_invalid_export_preserves_catalog(self):
        with patch.object(build,'ROOT',self.root),patch.object(build.subprocess,'run'),patch.object(build.subprocess,'check_output',return_value=b'invalid'):
            with self.assertRaises(ValueError):build.build()
        self.assertEqual(self.catalog.read_bytes(),self.original)

    def test_each_plugin_has_verified_identity_and_own_package(self):
        with patch.object(build,'ROOT',self.root),patch.object(build.subprocess,'run'),patch.object(build.subprocess,'check_output',return_value=b'[]'):
            build.build()
        catalog=json.loads(self.catalog.read_bytes());self.assertNotIn('runtime',catalog)
        urls=set()
        for plugin in catalog['plugins']:
            runtime=plugin['runtime'];urls.add(runtime['url'])
            archive=self.root/'release/plugins'/runtime['url'].rsplit('/',1)[1]
            self.assertEqual(build.digest(archive.read_bytes()),runtime['sha256'])
            with zipfile.ZipFile(archive) as z:
                self.assertEqual(json.loads(z.read('official-plugin.json'))['id'],plugin['id'])
                self.assertEqual(z.read('README.md').decode('utf-8'),runtime['readme'])
                self.assertEqual('voice-ui.js' in z.namelist(),plugin['id']=='voice')
        self.assertEqual(len(urls),8)

if __name__=='__main__':unittest.main()

import hashlib,json,sys,tempfile,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'scripts/build'))
from release_metadata import write_release_metadata

class ReleaseMetadataTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name)
        (self.root/'docs/release').mkdir(parents=True)
        (self.root/'docs/release/RELEASE_NOTES.md').write_text('发行说明',encoding='utf-8')
        self.output=self.root/'release/app';self.output.mkdir(parents=True)
        self.installer=self.output/'Shannon_1.0.1_x64-setup.exe';self.installer.write_bytes(b'fixture installer')
        self.signature=self.installer.with_name(self.installer.name+'.sig');self.signature.write_text('fixture signature')
        (self.root/'latest.json').write_text('{"version":"1.0.0"}')

    def test_update_and_upload_json_share_the_exact_verified_artifacts(self):
        latest,release=write_release_metadata(self.root,self.output,'1.0.1',self.installer,self.signature)
        self.assertEqual(latest['platforms']['windows-x86_64']['signature'],'fixture signature')
        self.assertTrue(latest['platforms']['windows-x86_64']['url'].endswith('/v1.0.1/'+self.installer.name))
        self.assertEqual((self.root/'latest.json').read_bytes(),(self.output/'latest.json').read_bytes())
        self.assertEqual(release['tag_name'],'v1.0.1')
        self.assertEqual(release['assets'][0]['sha256'],hashlib.sha256(self.installer.read_bytes()).hexdigest())
        self.assertEqual(json.loads((self.output/'release.json').read_text(encoding='utf-8')),release)

    def test_bad_plugin_archive_does_not_replace_published_update_manifest(self):
        (self.root/'src-tauri').mkdir();(self.root/'release/plugins').mkdir()
        (self.root/'release/plugins/note.zip').write_bytes(b'changed')
        (self.root/'src-tauri/official-plugins.json').write_text(json.dumps({'plugins':[{'runtime':{'url':'https://example.com/note.zip','sha256':'0'*64}}]}))
        before=(self.root/'latest.json').read_bytes()
        with self.assertRaises(ValueError):write_release_metadata(self.root,self.output,'1.0.1',self.installer,self.signature)
        self.assertEqual((self.root/'latest.json').read_bytes(),before)
        self.assertFalse((self.output/'release.json').exists())

    def test_empty_signature_does_not_create_release_json(self):
        self.signature.write_text('')
        with self.assertRaises(ValueError):write_release_metadata(self.root,self.output,'1.0.1',self.installer,self.signature)
        self.assertFalse((self.output/'release.json').exists())

    def test_two_sites_share_signed_bytes_but_use_their_own_download_urls(self):
        (self.root/'src-tauri').mkdir();(self.root/'release/plugins').mkdir()
        archive=self.root/'release/plugins/note.zip';archive.write_bytes(b'plugin')
        (self.root/'src-tauri/official-plugins.json').write_text(json.dumps({'plugins':[{'runtime':{
            'url':'https://gitee.com/wu-dinglin/shannon/releases/download/v1.0.1/note.zip',
            'sha256':hashlib.sha256(archive.read_bytes()).hexdigest()}}]}))
        write_release_metadata(self.root,self.output,'1.0.1',self.installer,self.signature)
        for site,host,branch,manifest in [('github','https://github.com/DinglinWu/Shannon','main','latest-github.json'),
                ('gitee','https://gitee.com/wu-dinglin/shannon','master','latest.json')]:
            folder=self.root/'release'/site
            latest=json.loads((folder/'latest.json').read_text(encoding='utf-8'))
            self.assertEqual(latest['platforms']['windows-x86_64']['url'],host+'/releases/download/v1.0.1/'+self.installer.name)
            self.assertEqual(latest['platforms']['windows-x86_64']['signature'],'fixture signature')
            self.assertEqual((folder/'latest.json').read_bytes(),(self.root/manifest).read_bytes())
            release=json.loads((folder/'release.json').read_text(encoding='utf-8'))
            self.assertEqual(release['site'],site)
            self.assertEqual(release['update_manifest']['publish_to'],branch+'/'+manifest)
            for item in release['assets']:
                path=self.root/item['path']
                self.assertEqual(path.parent,folder)
                self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(),item['sha256'])
        self.assertEqual((self.root/'release/github'/self.installer.name).read_bytes(),
                         (self.root/'release/gitee'/self.installer.name).read_bytes())

if __name__=='__main__':unittest.main()

import json,os,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'scripts/build'))
import signing
class SigningTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name);(self.root/'src-tauri').mkdir()
        (self.root/'src-tauri/tauri.conf.json').write_text(json.dumps({'bundle':{'windows':{}}}))
    def test_missing_key_fails_without_starting_a_build(self):
        with patch.dict(os.environ,{},clear=True):
            with self.assertRaisesRegex(ValueError,'私钥'):signing.updater_environment(self.root,self.root/'missing.key')
    def test_key_and_password_only_pass_through_child_environment(self):
        with patch.dict(os.environ,{'TAURI_SIGNING_PRIVATE_KEY':'fixture-only','TAURI_SIGNING_PRIVATE_KEY_PASSWORD':'fixture-password'},clear=True):
            env=signing.updater_environment(self.root)
            self.assertEqual(env['TAURI_SIGNING_PRIVATE_KEY_PASSWORD'],'fixture-password')
    def test_windows_signing_never_falls_back_when_required(self):
        with patch.dict(os.environ,{},clear=True):
            self.assertEqual(signing.windows_config(self.root),{})
            with self.assertRaisesRegex(ValueError,'Windows'):signing.windows_config(self.root,True)
        with patch.dict(os.environ,{'SHANNON_WINDOWS_CERT_THUMBPRINT':'a'*40},clear=True):
            self.assertEqual(signing.windows_config(self.root,True)['digestAlgorithm'],'sha256')
        with patch.dict(os.environ,{'SHANNON_WINDOWS_CERT_THUMBPRINT':'invalid'},clear=True):
            with self.assertRaises(ValueError):signing.windows_config(self.root)
if __name__=='__main__':unittest.main()

"""Updater signing and optional Windows Authenticode; never log secret values."""
import getpass
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile

def updater_environment(root, key_path=None):
    env=os.environ.copy()
    if not env.get('TAURI_SIGNING_PRIVATE_KEY'):
        path=Path(key_path or Path.home()/'.tauri/shannon.key').expanduser().resolve()
        if not path.is_file():
            raise ValueError(f'尚未生成更新签名私钥：{path}。请先按 docs/release/SIGNING.md 生成并配置公钥；本次没有构建无签名发布包。')
        env['TAURI_SIGNING_PRIVATE_KEY']=str(path)
    if 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD' not in env:
        if not sys.stdin.isatty():
            raise ValueError('请在本机交互终端运行打包并输入私钥密码；自动构建应通过安全环境变量提供密码，不要放进命令行。')
        env['TAURI_SIGNING_PRIVATE_KEY_PASSWORD']=getpass.getpass('更新签名私钥密码：')
    return env

def windows_config(root, required=False):
    config=json.loads((root/'src-tauri/tauri.conf.json').read_text(encoding='utf-8'))
    original=config.get('bundle',{}).get('windows',{})
    thumb=os.environ.get('SHANNON_WINDOWS_CERT_THUMBPRINT') or original.get('certificateThumbprint')
    if not thumb:
        if required:raise ValueError('缺少 Windows 代码签名证书：请配置 SHANNON_WINDOWS_CERT_THUMBPRINT。更新签名私钥不能替代 Windows 证书。')
        return {}
    thumb=re.sub(r'\s','',thumb).upper()
    if not re.fullmatch('[0-9A-F]{40}',thumb):raise ValueError('Windows 证书指纹必须为40位十六进制 SHA-1。')
    return {'certificateThumbprint':thumb,'digestAlgorithm':'sha256','timestampUrl':os.environ.get('SHANNON_WINDOWS_TIMESTAMP_URL') or original.get('timestampUrl') or 'http://timestamp.digicert.com','tsp':True}

def verify_windows(path, thumbprint=''):
    script=Path(__file__).with_name('verify_windows.ps1')
    command=['powershell.exe','-NoProfile','-NonInteractive','-File',str(script),'-Artifact',str(path)]
    if thumbprint:command+=['-ExpectedThumbprint',thumbprint]
    subprocess.run(command,check=True)

def sign_windows(path, config):
    tool=shutil.which('signtool.exe')
    if not tool:
        sdk=Path(os.environ.get('ProgramFiles(x86)','C:/Program Files (x86)'))/'Windows Kits/10/bin'
        candidates=sorted(sdk.glob('*/x64/signtool.exe'),key=lambda p:tuple(int(x) for x in p.parent.parent.name.split('.') if x.isdigit()))
        if candidates:tool=str(candidates[-1])
    if not tool:raise ValueError('未找到 Windows SDK 的 signtool.exe。')
    subprocess.run([tool,'sign','/sha1',config['certificateThumbprint'],'/fd','SHA256','/tr',config['timestampUrl'],'/td','SHA256',str(path)],check=True)
    verify_windows(path,config['certificateThumbprint'])

def verify_plugin_windows_signatures(root, config):
    catalog=json.loads((root/'src-tauri/official-plugins.json').read_text(encoding='utf-8'))
    checked=set()
    with tempfile.TemporaryDirectory(prefix='verify-signing-',dir=root/'release') as temp:
        for plugin in catalog['plugins']:
            spec=plugin['runtime']
            if spec['executable_sha256'] in checked:continue
            archive=root/'release/plugins'/spec['url'].rsplit('/',1)[1]
            with zipfile.ZipFile(archive) as package:data=package.read('runtime.exe')
            path=Path(temp)/'runtime.exe';path.write_bytes(data)
            verify_windows(path,config['certificateThumbprint'])
            checked.add(spec['executable_sha256'])

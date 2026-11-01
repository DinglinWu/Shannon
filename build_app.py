"""The public application build entry point. Run python build_app.py --help."""
import argparse
import re
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
ROOT=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT/'scripts/build'))
from signing import updater_environment, windows_config, verify_windows, verify_plugin_windows_signatures
from release_metadata import write_release_metadata
def main():
    parser=argparse.ArgumentParser(description='打包闪念主程序，默认要求更新签名，生成自动更新发布包')
    parser.add_argument('--exe-only',action='store_true',help='仅生成主程序可执行文件')
    parser.add_argument('--signed',action='store_true',help='兼容选项：现在默认就要求更新签名')
    parser.add_argument('--unsigned',action='store_true',help='仅本地测试，不生成自动更新发布信息')
    parser.add_argument('--updater-key',type=Path,help='更新签名私钥的本机路径')
    parser.add_argument('--require-windows-signature',action='store_true',help='未配置 Windows 证书时拒绝打包')
    parser.add_argument('--reuse-plugins',action='store_true',help='复用已经构建并校验的插件目录')
    parser.add_argument('--offline',action='store_true')
    parser.add_argument('--mirror',action='store_true')
    args=parser.parse_args()
    if args.unsigned and (args.signed or args.require_windows_signature or args.updater_key):parser.error('--unsigned 不与正式签名选项混用')
    if args.exe_only and not args.unsigned:parser.error('自动更新发布需要安装包；仅本地构建 EXE 请显式添加 --unsigned')
    try:
        win={} if args.unsigned else windows_config(ROOT,args.require_windows_signature)
        env=os.environ.copy() if args.unsigned else updater_environment(ROOT,args.updater_key)
        config=json.loads((ROOT/'src-tauri/tauri.conf.json').read_text(encoding='utf-8'))
        version=config['version']
        cargo_version=re.search(r'^version\s*=\s*"([^\"]+)"',(ROOT/'src-tauri/Cargo.toml').read_text(encoding='utf-8'),re.M).group(1)
        if version!=cargo_version or version!=json.loads((ROOT/'package.json').read_text(encoding='utf-8'))['version']:raise ValueError('三处版本号不一致')
        if not args.unsigned and (ROOT/'latest.json').exists():
            published=json.loads((ROOT/'latest.json').read_text(encoding='utf-8'))['version']
            if tuple(map(int,version.split('.'))) <= tuple(map(int,published.split('.'))):raise ValueError('正式更新版本必须高于 latest.json 中已有版本，请先更新版本号')
    except (ValueError,OSError) as error:parser.exit(1,str(error)+'\n')
    if win:env['SHANNON_WINDOWS_SIGNING_REQUIRED']='1'
    elif not args.unsigned:print('本次要求自动更新签名；尚未配置 Windows 证书，产物不会获得 Windows 发布者签名。')
    if not args.reuse_plugins:
        command=[sys.executable,str(ROOT/'build_plugins.py')]+['--'+n for n in ['offline','mirror'] if getattr(args,n)]
        plugin_env=os.environ.copy()
        if win:plugin_env['SHANNON_WINDOWS_SIGNING_REQUIRED']='1'
        subprocess.run(command,cwd=ROOT,env=plugin_env,check=True)
    subprocess.run(['node',str(ROOT/'scripts/build/build_frontend.mjs'),'core'],cwd=ROOT,check=True)
    subprocess.run([sys.executable,str(ROOT/'scripts/build/check_plugin_release.py')],cwd=ROOT,check=True)
    if win:verify_plugin_windows_signatures(ROOT,win)
    env['TAURI_CONFIG']=json.dumps({'build':{'frontendDist':'../dist-core'}})
    cargo_options=['--no-default-features']
    if args.offline:cargo_options+=['--offline']
    if args.mirror:cargo_options+=['--config','source.crates-io.replace-with="plugin-build-mirror"','--config','source.plugin-build-mirror.registry="sparse+https://rsproxy.cn/index/"']
    if args.exe_only:
        command=['cargo','build','--release','--manifest-path','src-tauri/Cargo.toml','--bin','shannon','--features','custom-protocol']+cargo_options
    else:
        command=['node',str(ROOT/'node_modules/@tauri-apps/cli/tauri.js'),'build','--config','src-tauri/tauri.core.conf.json','--config',json.dumps({'bundle':{'createUpdaterArtifacts':not args.unsigned,**({'windows':win} if win else {})}}),'--']+cargo_options
    started=__import__('time').time()
    subprocess.run(command,cwd=ROOT,env=env,check=True)
    output=ROOT/('release/dev' if args.unsigned else 'release/app');output.mkdir(parents=True,exist_ok=True)
    if not args.exe_only:
        version=json.loads((ROOT/'package.json').read_text(encoding='utf-8'))['version']
        installer=ROOT/f'src-tauri/target/release/bundle/nsis/Shannon_{version}_x64-setup.exe'
        if installer.stat().st_mtime<started:raise ValueError('安装包不是本次构建产物')
        if win:
            verify_windows(ROOT/'src-tauri/target/release/shannon.exe',win['certificateThumbprint'])
            verify_windows(installer,win['certificateThumbprint'])
        if not args.unsigned:
            signature=installer.with_name(installer.name+'.sig')
            if not signature.is_file() or signature.stat().st_mtime<started:raise ValueError('缺少本次构建的自动更新签名，不生成发布目录')
            subprocess.run(['node',str(ROOT/'scripts/build/verify_update_signature.mjs'),str(installer),str(signature),str(ROOT/'src-tauri/tauri.conf.json')],check=True)
            shutil.copy2(ROOT/'src-tauri/target/release/shannon.exe',output/'Shannon.exe')
            shutil.copy2(installer,output/installer.name)
            shutil.copy2(signature,output/signature.name)
            write_release_metadata(ROOT,output,version,output/installer.name,output/signature.name)
            print('已生成 release/github/ 和 release/gitee/，各含安装包、签名、官方插件、latest.json 和 release.json。先上传两站附件，再推送根目录 latest.json、latest-github.json。')
        else:
            shutil.copy2(installer,output/installer.name)
    if args.unsigned:shutil.copy2(ROOT/'src-tauri/target/release/shannon.exe',output/'Shannon.exe')
    print(f'主程序输出：{output}')
if __name__=='__main__':
    try:main()
    except subprocess.CalledProcessError as error:
        print(f'构建未完成（退出码 {error.returncode}），请查看上方错误信息；未发布发行版。',file=sys.stderr)
        raise SystemExit(error.returncode or 1)


"""Build separately installable official packages. Publish metadata only after success."""
import argparse, hashlib, json, os, subprocess, tempfile, zipfile, sys
from pathlib import Path
from plugin_readme import validate as validate_readme
from signing import windows_config, sign_windows
ROOT=Path(__file__).resolve().parents[2]
def digest(data): return hashlib.sha256(data).hexdigest()
def encoded(value): return (json.dumps(value,ensure_ascii=False,indent=2)+'\n').encode('utf-8')
def build(debug=False, offline=False, mirror=False):
    windows=windows_config(ROOT,os.environ.get('SHANNON_WINDOWS_SIGNING_REQUIRED')=='1')
    catalog_path=ROOT/'src-tauri/official-plugins.json'
    catalog=json.loads(catalog_path.read_text(encoding='utf-8'))
    previous=[p.get('runtime',{}) for p in catalog['plugins']]
    version=json.loads((ROOT/'package.json').read_text(encoding='utf-8'))['version']
    for plugin in catalog['plugins']:
        plugin['version']=version
        validate_readme((ROOT/'plugins/official'/plugin['id']/'README.md').read_text(encoding='utf-8'),plugin)
    subprocess.run(['node','scripts/build/build_frontend.mjs','runtime'],cwd=ROOT,check=True)
    env=os.environ.copy();env['SHANNON_PLUGIN_RUNTIME_BUILD']='1';env['TAURI_CONFIG']=json.dumps({'build':{'frontendDist':'../dist-runtime'}})
    args=['cargo','build','--manifest-path','src-tauri/Cargo.toml','--features','native-runtime,custom-protocol','--bin','shannon-plugin-runtime']
    if mirror: args += ['--config','source.crates-io.replace-with="plugin-build-mirror"','--config','source.plugin-build-mirror.registry="sparse+https://rsproxy.cn/index/"']
    if offline: args.append('--offline')
    if not debug: args.append('--release')
    subprocess.run(args,cwd=ROOT,env=env,check=True)
    executable=ROOT/'src-tauri/target'/('debug' if debug else 'release')/'shannon-plugin-runtime.exe'
    if windows:sign_windows(executable,windows)
    schemas=subprocess.check_output([str(executable),'--export-plugin-tools'],cwd=ROOT)
    json.loads(schemas.decode('utf-8-sig'))
    output=ROOT/'release/plugins';output.mkdir(parents=True,exist_ok=True)
    # Immutable content-addressed archives keep the old catalog usable even if
    # publication is interrupted. No catalog is changed during compilation.
    with tempfile.TemporaryDirectory(prefix='official-build-',dir=output) as temporary:
        stage=Path(temporary)
        exe=executable.read_bytes();ui=(ROOT/'src/voice-input.js').read_bytes()
        for plugin in catalog['plugins']:
            plugin['version']=version
            readme=validate_readme((ROOT/'plugins/official'/plugin['id']/'README.md').read_text(encoding='utf-8'),plugin)
            archive=stage/(plugin['id']+'.zip')
            with zipfile.ZipFile(archive,'w',compression=zipfile.ZIP_DEFLATED) as z:
                z.writestr('runtime.exe',exe)
                z.writestr('README.md',readme.encode('utf-8'))
                if plugin['id']=='voice':z.writestr('voice-ui.js',ui)
                z.writestr('official-plugin.json',encoded({'id':plugin['id'],'version':version,'protocol':1}))
            raw=archive.read_bytes();sha=digest(raw)
            filename=f"{plugin['id']}-{version}-{sha[:12]}-windows-x64.zip"
            archive.rename(stage/filename)
            plugin['runtime']={'id':plugin['id'],'protocol':1,'profile':'debug' if debug else 'release','version':version,'url':f'https://gitee.com/wu-dinglin/shannon/releases/download/v{version}/{filename}','bytes':len(raw),'sha256':sha,'executable_sha256':digest(exe)}
            plugin['runtime'].update(readme=readme,readme_sha256=digest(readme.encode('utf-8')),readme_metadata={key:plugin[key] for key in ['id','name','author','version','description']})
            if plugin['id']=='voice':plugin['runtime']['voice_ui_sha256']=digest(ui)
        catalog.pop('runtime',None)
        for archive in stage.glob('*.zip'):os.replace(archive,output/archive.name)
        for plugin in catalog['plugins']:
            (stage/(plugin['id']+'.json')).write_bytes(encoded(plugin))
            os.replace(stage/(plugin['id']+'.json'),output/f"{plugin['id']}-{version}.json")
        (stage/'tools.json').write_bytes(schemas)
        (stage/'catalog.json').write_bytes(encoded(catalog))
        # Install schemas first; the catalog is the final commit point.
        os.replace(stage/'tools.json',ROOT/'src-tauri/official-tools.json')
        os.replace(stage/'catalog.json',catalog_path)
    (output/'catalog.json').write_bytes(encoded(catalog))
    # Keep only the current distribution in plugins/; preserve superseded
    # packages in archive/ rather than silently deleting release history.
    current={p['runtime']['url'].rsplit('/',1)[1] for p in catalog['plugins']}
    archive_dir=ROOT/'release/archive/plugins'
    for spec in previous:
        name=spec.get('url','').rsplit('/',1)[-1]
        if not name or Path(name).name!=name or name in current:continue
        old=output/name
        if old.is_file() and digest(old.read_bytes())==spec.get('sha256'):
            archive_dir.mkdir(parents=True,exist_ok=True)
            assert old.resolve().is_relative_to(output.resolve())
            destination=archive_dir/name
            assert destination.resolve().is_relative_to((ROOT/'release').resolve())
            os.replace(old,destination)
    print(f'Built {len(catalog["plugins"])} official ZIPs in {output}. Nothing uploaded.')
if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--debug',action='store_true');parser.add_argument('--offline',action='store_true');parser.add_argument('--mirror',action='store_true');args=parser.parse_args()
    try:build(args.debug,args.offline,args.mirror)
    except subprocess.CalledProcessError as error:
        print(f'插件构建失败（退出码 {error.returncode}），请查看上方编译/网络错误。',file=sys.stderr)
        raise SystemExit(error.returncode or 1)

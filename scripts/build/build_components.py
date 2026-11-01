"""Build independently downloadable Windows components; never called by the base app build."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'release' / 'components'
STAGE = ROOT / 'release' / 'component-build'
COMPANION = Path(os.environ.get('GESTURE_MOUSE_HOME', r'E:\ironman\Gesture Recognition'))
VERSION = '1'


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def fresh(name):
    path = STAGE / name
    # All cleanup is constrained to this script's own build staging directory.
    if not path.resolve().is_relative_to(STAGE.resolve()):
        raise RuntimeError('Unsafe staging path')
    if path.exists():
        shutil.rmtree(path)
    path.mkdir(parents=True)
    return path


def package(name, path):
    files = {p.relative_to(path).as_posix(): digest(p) for p in sorted(path.rglob('*')) if p.is_file()}
    (path / 'component.json').write_text(json.dumps(dict(component=name, version=VERSION, files=files), ensure_ascii=False), encoding='utf-8')
    archive = OUT / f'{name}-{VERSION}-windows-x64.zip'
    total = 0
    with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED, compresslevel=6) as output:
        for item in sorted(path.rglob('*')):
            if item.is_file():
                total += item.stat().st_size
                output.write(item, item.relative_to(path).as_posix())
    app_version = json.loads((ROOT / 'package.json').read_text(encoding='utf-8'))['version']
    return dict(name=name, version=VERSION,
                url=f'https://gitee.com/wu-dinglin/shannon/releases/download/v{app_version}/{archive.name}',
                sha256=digest(archive), bytes=archive.stat().st_size, unpacked_bytes=total)


def voice():
    assets = ROOT / 'src-tauri' / 'voice-assets'
    if not (assets / 'model.int8.onnx').is_file():
        subprocess.run([sys.executable, str(ROOT / 'scripts/prepare_voice.py')], check=True)
    subprocess.run(['cargo', 'build', '--release', '--manifest-path', str(ROOT / 'src-tauri/Cargo.toml'),
                    '--bin', 'shannon-voice', '--features', 'voice-worker'], cwd=ROOT, check=True)
    stage = fresh('voice')
    shutil.copy2(ROOT / 'src-tauri/target/release/shannon-voice.exe', stage)
    (stage / 'assets').mkdir()
    for name in ('model.int8.onnx', 'tokens.txt', 'silero_vad.onnx'):
        shutil.copy2(assets / name, stage / 'assets' / name)
    shutil.copytree(ROOT / 'third_party/voice', stage / 'licenses')
    return package('voice', stage)


def gesture():
    stage = fresh('gesture')
    runtime = stage / 'python'
    runtime.mkdir()
    embedded = STAGE / 'python-3.12.8-embed-amd64.zip'
    if not embedded.exists():
        print('Downloading official portable Python 3.12.8…', flush=True)
        with urllib.request.urlopen('https://www.python.org/ftp/python/3.12.8/python-3.12.8-embed-amd64.zip', timeout=90) as source, embedded.with_suffix('.part').open('wb') as target:
            shutil.copyfileobj(source, target)
        embedded.with_suffix('.part').replace(embedded)
    with zipfile.ZipFile(embedded) as archive:
        # Official embedded archive has only direct filenames, no dynamic paths.
        for entry in archive.infolist():
            if '/' in entry.filename or '\\' in entry.filename or ':' in entry.filename:
                raise RuntimeError('Unexpected embedded Python archive path')
            archive.extract(entry, runtime)
    (runtime / 'python312._pth').write_text('python312.zip\n.\n..\nLib/site-packages\nimport site\n', encoding='utf-8')
    packages = COMPANION / '.venv/Lib/site-packages'
    shutil.copytree(packages, runtime / 'Lib/site-packages',
                    ignore=shutil.ignore_patterns('__pycache__', '*.pyc', 'pip', 'pip-*.dist-info'))
    names = ('actions.py','api_server.py','app.py','configuration.py','control.py','gestures.py','managed.py',
             'mouse_backend.py','pointer.py','recognition_feedback.py','shortcut_controller.py','state_machine.py','voice_point.py','wrist_controller.py')
    for name in names:
        shutil.copy2(COMPANION / name, stage / name)
    # MediaPipe's native file reader does not reliably open Unicode Windows paths.
    # Python opens the model and supplies bytes, so Chinese account names work.
    app = stage / 'app.py'
    app.write_text(app.read_text(encoding='utf-8').replace(
        'BaseOptions(model_asset_path=str(MODEL))',
        'BaseOptions(model_asset_buffer=MODEL.read_bytes())'), encoding='utf-8')
    # Keep the portable component independent from a developer's existing service.
    api = stage / 'api_server.py'
    api.write_text(api.read_text(encoding='utf-8').replace('PORT = 18765','PORT = 18766'), encoding='utf-8')
    config = json.loads((COMPANION / 'config.json').read_text(encoding='utf-8'))
    config['camera'] = 0
    (stage / 'config.json').write_text(json.dumps(config, indent=2), encoding='utf-8')
    shutil.copytree(COMPANION / 'models', stage / 'models')
    for name in ('README.md','requirements.txt','LICENSE'):
        if (COMPANION / name).is_file():
            shutil.copy2(COMPANION / name, stage / name)
    # Validate the portable interpreter and libraries without opening a camera.
    subprocess.run([str(runtime / 'python.exe'), str(stage / 'app.py'), '--check'], cwd=stage, check=True)
    return package('gesture', stage)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--only', choices=['voice', 'gesture'])
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    STAGE.mkdir(parents=True, exist_ok=True)
    catalog_path = ROOT / 'src-tauri/component-catalog.json'
    old = json.loads(catalog_path.read_text(encoding='utf-8'))
    components = {item['name']: item for item in old['components']}
    for name, build in (('voice', voice), ('gesture', gesture)):
        if args.only is None or args.only == name:
            print(f'Building optional {name} component…', flush=True)
            built = build()
            components = {item['name']:item for item in json.loads(catalog_path.read_text(encoding='utf-8'))['components']}
            components[name] = built
            catalog_path.write_text(json.dumps(dict(schema=1, components=list(components.values())), indent=2) + '\n', encoding='utf-8')
    print('Component archives and pinned catalog prepared. Upload ZIP files with the matching application release.', flush=True)


if __name__ == '__main__':
    main()

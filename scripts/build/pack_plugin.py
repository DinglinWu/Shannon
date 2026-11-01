"""Pack a 闪念 JS tool plugin: python scripts/build/pack_plugin.py examples/plugins/quick-stats"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import zipfile
from plugin_readme import validate as validate_readme

def main():
    parser = argparse.ArgumentParser(description="打包闪念工具插件")
    parser.add_argument("folder", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    folder = args.folder.resolve()
    manifest = json.loads((folder / "plugin.json").read_text(encoding="utf-8"))
    readme=validate_readme((folder/"README.md").read_text(encoding="utf-8"),manifest)
    plugin_id, version = manifest["id"], manifest["version"]
    if not re.fullmatch(r"[a-z][a-z0-9-]{0,23}", plugin_id) or not re.fullmatch(r"[0-9]{1,5}\.[0-9]{1,5}\.[0-9]{1,5}", version):
        raise ValueError("插件ID或版本无效")
    entry = (folder / manifest["entry"]).resolve()
    if not entry.is_relative_to(folder) or not entry.is_file() or entry.suffix != ".js":
        raise ValueError("入口必须为插件目录内的JavaScript文件")
    output = (args.output or Path(__file__).resolve().parents[2] / "release/plugins" / f"{plugin_id}-{version}.zip").resolve()
    if output.is_relative_to(folder):
        raise ValueError("输出ZIP不能放进插件源目录")
    output.parent.mkdir(parents=True, exist_ok=True)
    # Runtime v1 consumes only the manifest and one bundled JS entry.
    with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.write(folder / "plugin.json", "plugin.json")
        archive.writestr("README.md",readme.encode("utf-8"))
        archive.write(entry, entry.relative_to(folder).as_posix())
    print(f"已生成：{output}")
    print(f"SHA-256：{hashlib.sha256(output.read_bytes()).hexdigest()}")
    print(f"安装：/plugin install {output}")

if __name__ == "__main__":
    main()

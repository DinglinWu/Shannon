"""The public plugin build entry point. Run python build_plugins.py --help."""
import argparse
import json
import subprocess
import sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent
def write_index():
    output=ROOT/'release/plugins'
    catalog=json.loads((output/'catalog.json').read_text(encoding='utf-8'))
    lines=['# 当前插件包','','所有 ZIP 均包含 README.md；官方包通过插件管理窗口安装。','','| 插件 | 开发者 | 版本 | ZIP |','| --- | --- | --- | --- |']
    for plugin in catalog['plugins']:
        name=plugin['runtime']['url'].rsplit('/',1)[1]
        lines.append(f"| {plugin['name']} | {plugin['author']} | {plugin['version']} | [{name}]({name}) |")
    lines+=['','第三方示例：[quick-stats-1.0.0.zip](quick-stats-1.0.0.zip)','','旧包保留在 `../archive/plugins/`。当前包的完整校验值见 catalog.json。']
    (output/'INDEX.md').write_text('\n'.join(lines)+'\n',encoding='utf-8')
def main():
    parser=argparse.ArgumentParser(description='打包官方插件和第三方 JS 插件')
    parser.add_argument('folder',nargs='?',type=Path,help='第三方插件目录；省略时构建八个官方插件及示例包')
    parser.add_argument('--debug',action='store_true')
    parser.add_argument('--offline',action='store_true')
    parser.add_argument('--mirror',action='store_true')
    parser.add_argument('--output',type=Path,help='仅用于单个第三方插件')
    args=parser.parse_args()
    if args.folder:
        if args.debug or args.offline or args.mirror:parser.error('单个 JS 插件无需 Rust 构建选项')
        command=[sys.executable,str(ROOT/'scripts/build/pack_plugin.py'),str(args.folder.resolve())]
        if args.output:command+=['--output',str(args.output.resolve())]
        subprocess.run(command,cwd=ROOT,check=True)
    else:
        if args.output:parser.error('--output 仅用于单个第三方插件')
        command=[sys.executable,str(ROOT/'scripts/build/build_official_plugins.py')]
        command += ['--'+name for name in ['debug','offline','mirror'] if getattr(args,name)]
        subprocess.run(command,cwd=ROOT,check=True)
        subprocess.run([sys.executable,str(ROOT/'scripts/build/pack_plugin.py'),str(ROOT/'examples/plugins/quick-stats')],cwd=ROOT,check=True)
        write_index()
if __name__=='__main__':
    try:main()
    except subprocess.CalledProcessError as error:
        print(f'构建未完成（退出码 {error.returncode}），请查看上方错误信息；未发布发行版。',file=sys.stderr)
        raise SystemExit(error.returncode or 1)


# %% [markdown]
# # 闪念在线主题发布脚本
#
# 1. 校验项目根目录 themes/ 下的每个主题
# 2. 生成主题清单 themes/index.json（列出每个文件的大小与 SHA-256）
# 3. 使用发布安装包的同一把私钥签名，生成 themes/index.json.sig
#
# 执行完成后提交并推送 themes/ 目录。已安装的闪念将在下次检查时同步（默认每 6 小时），
# 也可在闪念中执行 /theme sync 立即同步。详见 RELEASE.md。

# %% 参数
PROJECT_DIR = r"E:\ironman\ai-launcher-v2\ai-launcher-v2"   # 项目根目录
PRIVATE_KEY = r"%USERPROFILE%\.tauri\shannon.key"           # 签名私钥
THEMES_DIR = "themes"                                       # 在线主题目录（项目根目录下）
SIGN = True                                                 # False：仅校验并生成清单，不签名

# %% 校验主题并生成清单
import datetime
import getpass
import hashlib
import json
import os
import pathlib
import re
import subprocess
import time

# 与程序中的校验规则保持一致（src-tauri/src/theme.rs、online_theme.rs、src/theme.js）
ID_RE = re.compile(r"^[a-z0-9][a-z0-9-]{0,47}$")
FILE_RE = re.compile(r"^[A-Za-z0-9_-][A-Za-z0-9_.-]{0,63}\.(svg|png|jpe?g|webp|gif)$", re.I)
IGNORED = {"thumbs.db", "desktop.ini", ".ds_store"}
MAX_MANIFEST = 64 * 1024
MAX_ASSET = 2 * 1024 * 1024
MAX_THEME = 8 * 1024 * 1024
MAX_FILES = 16
MAX_THEMES = 32

root = pathlib.Path(PROJECT_DIR)
themes_dir = root / THEMES_DIR
themes_dir.mkdir(exist_ok=True)
today = datetime.date.today().isoformat()


def check_theme(tdir: pathlib.Path):
    """返回 (清单条目, 错误列表, 提示列表)"""
    errors, notes, files = [], [], {}
    tid = tdir.name
    if not ID_RE.match(tid):
        errors.append("文件夹名仅可包含小写字母、数字和连字符")

    for f in sorted(tdir.iterdir()):
        if f.name.lower() in IGNORED:
            continue
        if f.is_dir():
            errors.append(f"不支持子文件夹：{f.name}")
            continue
        if f.name != "theme.json" and not FILE_RE.match(f.name):
            errors.append(f"不支持的文件：{f.name}（仅允许 theme.json 与 svg/png/jpg/webp/gif 图片）")
            continue
        data = f.read_bytes()
        limit = MAX_MANIFEST if f.name == "theme.json" else MAX_ASSET
        if len(data) > limit:
            errors.append(f"{f.name} 超过大小上限（{limit // 1024} KB）")
        files[f.name] = {"size": len(data), "sha256": hashlib.sha256(data).hexdigest()}

    if "theme.json" not in files:
        errors.append("缺少 theme.json")
        return None, errors, notes
    if len(files) > MAX_FILES:
        errors.append(f"文件数超过上限 {MAX_FILES}")
    if sum(x["size"] for x in files.values()) > MAX_THEME:
        errors.append("主题总大小超过 8 MB")

    try:
        manifest = json.loads((tdir / "theme.json").read_text(encoding="utf-8-sig"))
    except Exception as e:
        errors.append(f"theme.json 格式错误：{e}")
        return None, errors, notes
    if manifest.get("spec") != 1:
        errors.append("theme.json 的 spec 应为 1")
    if manifest.get("id", tid) != tid:
        errors.append(f"theme.json 的 id 与文件夹名不一致")

    # 引用的图片须存在于主题文件夹中
    slots = manifest.get("slots") or {}
    refs = [(slots.get(k) or {}).get("image") for k in ("badge", "pattern")]
    refs += ((manifest.get("enter") or {}).get("params") or {}).get("images") or []
    for r in refs:
        if r and r not in files:
            errors.append(f"theme.json 引用的图片不存在：{r}")

    when = manifest.get("when") or {}
    if len(when.get("to", "")) == 10 and when["to"] < today:
        notes.append(f"生效日期已过（{when['to']}），可从 themes/ 中删除")
    if not when:
        notes.append("未设置 when，不会自动启用，仅可通过 /theme 名称 手动启用")

    return {"id": tid, "files": files}, errors, notes


entries, failed = [], False
for tdir in sorted(p for p in themes_dir.iterdir() if p.is_dir()):
    entry, errors, notes = check_theme(tdir)
    status = "✗" if errors else "✓"
    print(f"{status} {tdir.name}")
    for e in errors:
        print(f"    错误：{e}")
    for n in notes:
        print(f"    提示：{n}")
    if errors:
        failed = True
    elif entry:
        entries.append(entry)

assert not failed, "存在未通过校验的主题，请修正后重新执行"
assert len(entries) <= MAX_THEMES, f"主题数量超过上限 {MAX_THEMES}"

index = {"spec": 1, "generated": int(time.time()), "themes": entries}
index_path = themes_dir / "index.json"
# 以二进制写入，确保签名对象与推送到仓库的内容逐字节一致
index_path.write_bytes((json.dumps(index, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))
print(f"\n已生成清单：{index_path}（{len(entries)} 个主题）")

# %% 签名
if SIGN:
    key_path = pathlib.Path(os.path.expandvars(PRIVATE_KEY))
    assert key_path.exists(), f"未找到私钥：{key_path}"
    env = os.environ.copy()
    env["TAURI_SIGNING_PRIVATE_KEY"] = key_path.read_text(encoding="utf-8").strip()
    env["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] = getpass.getpass("私钥密码：")
    sig_path = index_path.with_name("index.json.sig")
    sig_path.unlink(missing_ok=True)
    subprocess.run(
        f'npx tauri signer sign "{index_path}"',
        cwd=root, env=env, shell=True, check=True,
        stdout=subprocess.DEVNULL,
    )
    assert sig_path.exists(), f"未生成签名文件：{sig_path}"
    print(f"已签名：{sig_path}")
    print("\n下一步：提交并推送 themes/ 目录")
    print('  git add themes; git commit -m "更新在线主题"; git push')

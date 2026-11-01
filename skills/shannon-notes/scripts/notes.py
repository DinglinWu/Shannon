"""Dependency-free local client for the running 闪念 notes bridge."""
import argparse
import json
import os
from pathlib import Path
import sys
import time
import uuid


def call(config_dir, request, timeout=12):
    directory = Path(config_dir) / "notes-bridge"
    if not directory.is_dir():
        raise RuntimeError("便签接口不存在，请先启动包含技能接口的新版闪念。")
    key = uuid.uuid4().hex
    pending = directory / (key + ".request.json")
    temporary = directory / (key + ".request.tmp")
    response = directory / (key + ".response.json")
    envelope = {"expires_at": int(time.time()) + timeout, "request": request}
    try:
        temporary.write_text(json.dumps(envelope, ensure_ascii=False), encoding="utf-8")
        temporary.replace(pending)
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if response.exists():
                result = json.loads(response.read_text(encoding="utf-8"))
                response.unlink()
                return result
            time.sleep(0.1)
        raise RuntimeError("调用超时，结果未知。确认新版闪念正在运行，先 list 核对，不要直接重复添加。")
    finally:
        temporary.unlink(missing_ok=True)
        pending.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["add", "list", "show", "close", "complete", "restore"])
    parser.add_argument("--text")
    parser.add_argument("--text-file", type=Path)
    parser.add_argument("--id", type=int)
    parser.add_argument("--config-dir", type=Path)
    args = parser.parse_args()
    request = {"action": args.action}
    if args.action == "add":
        if (args.text is None) == (args.text_file is None):
            parser.error("add 需要且只能使用 --text 或 --text-file 之一")
        text = args.text_file.read_text(encoding="utf-8-sig") if args.text_file else args.text
        if not text.strip() or len(text.strip()) > 2000:
            parser.error("便签内容须为 1–2000 字")
        request["text"] = text.strip()
    if args.action in ("complete", "restore"):
        if args.id is None or args.id < 1:
            parser.error("需要有效的 --id，请先 list 获取")
        request["id"] = args.id
    config_dir = args.config_dir
    if config_dir is None:
        if not os.environ.get("APPDATA"):
            parser.error("未找到 APPDATA，请指定 --config-dir")
        config_dir = Path(os.environ["APPDATA"]) / "com.brynn.ailauncher"
    result = call(config_dir, request)
    print(json.dumps(result, ensure_ascii=False))
    return 0 if result.get("ok") else 1


if __name__ == "__main__":
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    try:
        sys.exit(main())
    except (OSError, ValueError, RuntimeError) as error:
        print(json.dumps({"ok": False, "error": str(error)}, ensure_ascii=False))
        sys.exit(1)

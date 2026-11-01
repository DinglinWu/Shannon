"""Prepare pinned offline speech assets for development and Tauri packaging."""
import hashlib
import json
import platform
from pathlib import Path
import shutil
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
DEST = ROOT / "src-tauri" / "voice-assets"
MODEL_URL = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17.tar.bz2"
VAD_URL = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx"
NATIVE_NAME = "sherpa-onnx-v1.13.8-win-x64-static-MD-Release-no-tts-lib.tar.bz2"
NATIVE_SHA256 = "542348e56e827b59c6d249fd0dfd38dc34b7bd0c521a1ebe9eb6e2db154fa15d"
EXPECTED = {
    "model.int8.onnx": "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51",
    "tokens.txt": "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc",
    "silero_vad.onnx": "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6",
    "zh.wav": "b77f1794fe374a0ba1ee1dc458bfaf9349496cbbfc32780c50ba3c5a7ad8e373",
}


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def valid(name):
    path = DEST / name
    return path.is_file() and digest(path) == EXPECTED[name]


def download(url, path):
    print(f"Downloading {path.name}…", flush=True)
    with urllib.request.urlopen(url, timeout=90) as source, path.open("wb") as target:
        shutil.copyfileobj(source, target)


def main():
    DEST.mkdir(parents=True, exist_ok=True)
    archive = DEST / "sensevoice.tar.bz2.part"
    if not all(valid(name) for name in ("model.int8.onnx", "tokens.txt", "zh.wav")):
        download(MODEL_URL, archive)
        # Extract only these known files, never archive paths or links.
        with tarfile.open(archive, "r:bz2") as source:
            for entry in source:
                name = Path(entry.name).name
                if entry.isfile() and name in ("model.int8.onnx", "tokens.txt", "LICENSE", "zh.wav"):
                    target = DEST / name
                    with source.extractfile(entry) as data, target.with_suffix(target.suffix + ".part").open("wb") as output:
                        shutil.copyfileobj(data, output)
                    temporary = target.with_suffix(target.suffix + ".part")
                    if name in EXPECTED and digest(temporary) != EXPECTED[name]:
                        raise RuntimeError(f"Checksum mismatch: {name}")
                    temporary.replace(target)
        archive.unlink()
    if not valid("silero_vad.onnx"):
        download(VAD_URL, DEST / "silero_vad.onnx.part")
        if digest(DEST / "silero_vad.onnx.part") != EXPECTED["silero_vad.onnx"]:
            raise RuntimeError("Checksum mismatch: silero_vad.onnx")
        (DEST / "silero_vad.onnx.part").replace(DEST / "silero_vad.onnx")
    if not all(valid(name) for name in EXPECTED):
        raise RuntimeError("Speech resources are incomplete")
    manifest = {name: digest(DEST / name) for name in EXPECTED}
    (DEST / "checksums.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    if platform.system() == "Windows" and platform.machine().lower() in ("amd64", "x86_64"):
        native_dir = ROOT / "src-tauri" / "target" / "sherpa-onnx-prebuilt"
        native_dir.mkdir(parents=True, exist_ok=True)
        native_archive = native_dir / NATIVE_NAME
        if not native_archive.is_file() or digest(native_archive) != NATIVE_SHA256:
            temporary = native_archive.with_suffix(".download")
            download(f"https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.13.8/{NATIVE_NAME}", temporary)
            if digest(temporary) != NATIVE_SHA256:
                raise RuntimeError("Checksum mismatch: native speech runtime")
            temporary.replace(native_archive)
        # The pinned Cargo build script performs extraction when compilation begins.
        print("Native Windows runtime verified.", flush=True)
    print(json.dumps(manifest, indent=2), flush=True)


if __name__ == "__main__":
    main()

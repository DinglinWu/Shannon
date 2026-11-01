"""Compatibility shim: all release behavior belongs to the root entry point."""
from pathlib import Path
import subprocess,sys
if __name__=='__main__':
    root=Path(__file__).resolve().parents[2]
    raise SystemExit(subprocess.call([sys.executable,str(root/'build_app.py'),*sys.argv[1:]],cwd=root))

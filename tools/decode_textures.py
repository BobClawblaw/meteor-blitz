#!/usr/bin/env python3
"""Decode the original PVRTC textures (original/app/*.pvr) to PNG.

Builds tools/pvr_decode.cpp against PowerVR's reference decoder the first
time it runs. The decoder source is expected at --pvrt-dir (default: the
touchHLE checkout's vendor/PVRTDecompress).

Usage: tools/decode_textures.py [--pvrt-dir DIR] [--out DIR]
Default output: original/textures.

History: the first decode of these files (with the texture2ddecoder Python
package) had red and blue swapped; this decoder is the reference one and
its output matches that decode exactly once the channels are swapped back.
"""

import argparse
import subprocess
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "original" / "app"


def build(pvrt_dir: Path, build_dir: Path) -> Path:
    exe = build_dir / "pvr_decode"
    cpp = ROOT / "tools" / "pvr_decode.cpp"
    dec = pvrt_dir / "PVRTDecompress.cpp"
    if exe.exists() and exe.stat().st_mtime > max(cpp.stat().st_mtime, dec.stat().st_mtime):
        return exe
    build_dir.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["clang++", "-O2", "-std=c++17", "-I", str(pvrt_dir), str(cpp), str(dec), "-o", str(exe)],
        check=True,
    )
    return exe


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pvrt-dir", default=str(Path.home() / "tools/touchHLE-src/vendor/PVRTDecompress"))
    ap.add_argument("--out", default=str(ROOT / "original" / "textures"))
    ap.add_argument("--build-dir", default=str(ROOT / "target" / "pvr_decode"))
    args = ap.parse_args()

    exe = build(Path(args.pvrt_dir), Path(args.build_dir))
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    raw = Path(args.build_dir) / "tmp.rgba"
    for pvr in sorted(SRC.glob("*.pvr")):
        info = subprocess.run([str(exe), str(pvr), str(raw)], check=True, capture_output=True, text=True).stdout
        w, h, flipped = (int(v) for v in info.split())
        im = Image.frombytes("RGBA", (w, h), raw.read_bytes())
        if flipped:
            im = im.transpose(Image.FLIP_TOP_BOTTOM)
        name = pvr.name.split(".")[0] + ".png"
        im.save(out / name)
        print(f"{name}: {w}x{h} flipped={flipped}")
    raw.unlink(missing_ok=True)


if __name__ == "__main__":
    sys.exit(main())

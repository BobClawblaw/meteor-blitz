#!/usr/bin/env python3
"""Decode the sprite sheet original/app/game.raw into original/sprites/.

game.raw is a 16-byte header (width 128, height 128, unknown, frame size
4096) followed by 520 headerless PVRTC 2bpp frames of 128x128. The decoder
binary is built by tools/decode_textures.py (PowerVR reference decoder).

Usage: tools/decode_sprites.py [--out DIR]
"""

import argparse
import struct
import subprocess
import sys
from pathlib import Path

from PIL import Image

from decode_textures import build

ROOT = Path(__file__).resolve().parent.parent
SHEET = ROOT / "original" / "app" / "game.raw"
HEADER = 16
FRAME_BYTES = 4096
FRAMES = 520
SIZE = 128


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pvrt-dir", default=str(Path.home() / "tools/touchHLE-src/vendor/PVRTDecompress"))
    ap.add_argument("--out", default=str(ROOT / "original" / "sprites"))
    ap.add_argument("--build-dir", default=str(ROOT / "target" / "pvr_decode"))
    args = ap.parse_args()

    width, height, _, frame_bytes = struct.unpack("<4I", SHEET.read_bytes()[:HEADER])
    if (width, height, frame_bytes) != (SIZE, SIZE, FRAME_BYTES):
        sys.exit(f"unexpected sheet header {width}x{height}, frame bytes {frame_bytes}")
    exe = build(Path(args.pvrt_dir), Path(args.build_dir))
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    raw = Path(args.build_dir) / "tmp.rgba"
    for i in range(FRAMES):
        offset = HEADER + i * FRAME_BYTES
        subprocess.run(
            [str(exe), "--raw", str(SIZE), str(SIZE), "2", str(offset), str(SHEET), str(raw)],
            check=True,
            capture_output=True,
        )
        Image.frombytes("RGBA", (SIZE, SIZE), raw.read_bytes()).save(out / f"frame_{i:03}.png")
    raw.unlink(missing_ok=True)
    print(f"decoded {FRAMES} frames into {out}")


if __name__ == "__main__":
    sys.exit(main())

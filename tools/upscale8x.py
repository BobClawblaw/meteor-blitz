#!/usr/bin/env python3
"""Build the experimental 8x art set from the committed 4x set.

- UI, sprites and the bitmap font: xBR 2x on the 4x pixel art, so the
  result is 8x the original. The font's glyph rectangles are scaled by 8.
- Terrain and fog stay at the committed 2x HD size: 4x terrain (2048x2048)
  minified without mipmaps showed vertical aliasing bands at 4K, while 2x
  holds 60 Hz and wraps cleanly. The 8x set copies the 4x textures and
  normal maps.
- The normal maps are the committed 2x ones, copied alongside.

The game loads this set with MB_HD=8. It is about 143 MB and gives 8x
sprites, UI and font with the proven 2x terrain.
Usage:
  tools/upscale8x.py [--only ui|sprites|font|textures|normals|all]
"""

import argparse
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "assets" / "4x"
OUT = ROOT / "assets" / "8x"
FFMPEG = shutil.which("ffmpeg")

sys.path.insert(0, str(Path(__file__).resolve().parent))


def xbr_scale(src: Path, dst: Path) -> None:
    """xBR 2x, preserving alpha by scaling it as a separate mask."""
    im = Image.open(src)
    dst.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        if im.mode == "RGBA":
            rgb = im.convert("RGB")
            alpha = im.getchannel("A")
            rgb_path = tmp / "rgb.png"
            alpha_path = tmp / "alpha.png"
            rgb.save(rgb_path)
            alpha.save(alpha_path)
            rgb2 = tmp / "rgb2.png"
            alpha2 = tmp / "alpha2.png"
            run_xbr(rgb_path, rgb2)
            run_xbr(alpha_path, alpha2)
            out = Image.open(rgb2).convert("RGB")
            out.putalpha(Image.open(alpha2).convert("L"))
        else:
            rgb_path = tmp / "rgb.png"
            rgb2 = tmp / "rgb2.png"
            im.convert("RGB").save(rgb_path)
            run_xbr(rgb_path, rgb2)
            out = Image.open(rgb2).convert(im.mode)
    out.save(dst, optimize=True)


def run_xbr(src: Path, dst: Path) -> None:
    subprocess.run(
        [FFMPEG, "-hide_banner", "-loglevel", "error", "-i", str(src),
         "-vf", "xbr=n=2", "-y", str(dst)],
        check=True,
    )


def upscale_dir(src_dir: Path, dst_dir: Path, label: str) -> None:
    for p in sorted(src_dir.glob("*.png")):
        dst = dst_dir / p.name
        xbr_scale(p, dst)
        print(label, p.name, Image.open(p).size, "->", Image.open(dst).size)


def upscale_font() -> None:
    src = SRC / "ngage.png"
    dst = OUT / "ngage.png"
    xbr_scale(src, dst)
    dat = (ROOT / "original" / "app" / "ngage.dat").read_bytes()
    count = int.from_bytes(dat[0:2], "little")
    out = bytearray(dat[0:2])
    for i in range(count):
        for k in range(4):
            at = 2 + i * 8 + k * 2
            v = int.from_bytes(dat[at:at + 2], "little") * 8
            out += v.to_bytes(2, "little")
    (OUT / "ngage.dat").write_bytes(out)
    print("font", Image.open(src).size, "->", Image.open(dst).size)


def copy_textures() -> None:
    for p in sorted((SRC / "textures").glob("*.png")):
        dst = OUT / "textures" / p.name
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(p, dst)
        print("texture", p.name, Image.open(p).size, "copied")


def copy_normals() -> None:
    for p in sorted((SRC / "normals").glob("*.png")):
        dst = OUT / "normals" / p.name
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(p, dst)
        print("normal", p.name, Image.open(p).size, "copied")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--only", choices=["ui", "sprites", "font", "textures", "normals", "all"],
                    default="all")
    args = ap.parse_args()
    if FFMPEG is None:
        sys.exit("ffmpeg is required")
    if args.only in ("ui", "all"):
        upscale_dir(SRC / "ui", OUT / "ui", "ui")
    if args.only in ("sprites", "all"):
        upscale_dir(SRC / "sprites", OUT / "sprites", "sprite")
    if args.only in ("font", "all"):
        upscale_font()
    if args.only in ("textures", "all"):
        copy_textures()
    if args.only in ("normals", "all"):
        copy_normals()


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Upscale the original graphics for the HD render path.

- Terrain/fog/space/horizon textures: Real-ESRGAN x4plus when the ncnn
  binary is available, otherwise Lanczos, at 2x. The backdrop mesh samples
  these minified across the screen and macroquad builds no mipmaps, so a 4x
  copy (2048x2048) stalls the GPU and drops the frame rate; 2x holds 60 Hz.
- UI art, sprites and the bitmap font: xBR 4x (pixel-art aware), with the
  alpha channel upscaled separately so transparency survives.

Outputs mirror the source layout under assets/4x/ and are regenerable.
Usage:
  tools/upscale4x.py [--esrgan DIR] [--only textures|ui|sprites|font|all]
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
OUT = ROOT / "assets" / "4x"
FFMPEG = shutil.which("ffmpeg")
DEFAULT_ESRGAN = Path("/tmp/resrgan/realesrgan-ncnn-vulkan")


def xbr4(src: Path, dst: Path) -> None:
    """xBR 4x, preserving alpha by scaling it as a separate mask."""
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
            rgb4 = tmp / "rgb4.png"
            alpha4 = tmp / "alpha4.png"
            run_xbr(rgb_path, rgb4)
            run_xbr(alpha_path, alpha4)
            out = Image.open(rgb4).convert("RGB")
            mask = Image.open(alpha4).convert("L")
            out.putalpha(mask)
        else:
            rgb_path = tmp / "rgb.png"
            im.convert("RGB").save(rgb_path)
            rgb4 = tmp / "rgb4.png"
            run_xbr(rgb_path, rgb4)
            out = Image.open(rgb4).convert(im.mode)
    out.save(dst, optimize=True)


def run_xbr(src: Path, dst: Path) -> None:
    subprocess.run(
        [FFMPEG, "-hide_banner", "-loglevel", "error", "-i", str(src),
         "-vf", "xbr=n=4", "-y", str(dst)],
        check=True,
    )


def esrgan_upscale(src: Path, dst: Path, esrgan: Path, scale: int) -> bool:
    if not esrgan.exists():
        return False
    dst.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [str(esrgan), "-i", str(src), "-o", str(dst), "-s", str(scale),
         "-m", str(esrgan.parent / "models"), "-n", "realesrgan-x4plus",
         "-f", "png"],
        check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    return True


def lanczos(src: Path, dst: Path, scale: int) -> None:
    im = Image.open(src)
    dst.parent.mkdir(parents=True, exist_ok=True)
    im.resize((im.width * scale, im.height * scale), Image.LANCZOS).save(dst, optimize=True)


def verify(src: Path, dst: Path) -> float:
    a = np.asarray(Image.open(src).convert("RGB"), dtype=np.float32)
    b = Image.open(dst).convert("RGB")
    b = b.resize((a.shape[1], a.shape[0]), Image.LANCZOS)
    b = np.asarray(b, dtype=np.float32)
    return float(np.abs(a - b).mean())


def upscale_textures(esrgan: Path) -> None:
    # 2x: the backdrop mesh minifies these and macroquad has no mipmaps.
    src = ROOT / "original" / "textures"
    for p in sorted(src.glob("*.png")):
        dst = OUT / "textures" / p.name
        if not esrgan_upscale(p, dst, esrgan, 2):
            lanczos(p, dst, 2)
        print("texture", p.name, "->", dst.relative_to(ROOT), "diff", round(verify(p, dst), 2))


def upscale_dir(src: Path, dst_dir: Path, label: str) -> None:
    for p in sorted(src.glob("*.png")):
        dst = dst_dir / p.name
        xbr4(p, dst)
        print(label, p.name, "diff", round(verify(p, dst), 2))


def upscale_font() -> None:
    src = ROOT / "assets" / "ui" / "ngage.png"
    dst = OUT / "ngage.png"
    xbr4(src, dst)
    dat = (ROOT / "original" / "app" / "ngage.dat").read_bytes()
    count = int.from_bytes(dat[0:2], "little")
    out = bytearray(dat[0:2])
    for i in range(count):
        for k in range(4):
            at = 2 + i * 8 + k * 2
            v = int.from_bytes(dat[at:at + 2], "little") * 4
            out += v.to_bytes(2, "little")
    (OUT / "ngage.dat").write_bytes(out)
    print("font", "->", dst.relative_to(ROOT), "diff", round(verify(src, dst), 2))


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--esrgan", type=Path, default=DEFAULT_ESRGAN)
    ap.add_argument("--only", choices=["textures", "ui", "sprites", "font", "all"],
                    default="all")
    args = ap.parse_args()
    if FFMPEG is None:
        sys.exit("ffmpeg is required")
    if args.only in ("textures", "all"):
        upscale_textures(args.esrgan)
    if args.only in ("ui", "all"):
        upscale_dir(ROOT / "assets" / "ui", OUT / "ui", "ui")
    if args.only in ("sprites", "all"):
        upscale_dir(ROOT / "original" / "sprites", OUT / "sprites", "sprite")
    if args.only in ("font", "all"):
        upscale_font()


if __name__ == "__main__":
    main()

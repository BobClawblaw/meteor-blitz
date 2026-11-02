#!/usr/bin/env python3
"""Build the 8x art set (the desktop default) from the originals.

- UI, sprites and the bitmap font: xBR 2x on the 4x set's xBR 4x of the
  originals (ffmpeg's xBR stops at 4x), so the result is 8x the original.
  The font's glyph rectangles are scaled by 8.
- Terrain and fog: 4096x4096 from the 512x512 originals, through
  upscale4x's `terrain`: Real-ESRGAN x4plus twice on the texture wrapped in
  padding, brought down to 8x and cropped, so it tiles with full detail to
  its edges; `close_seam` evens out the step some originals have at their
  wrap, the fogs use Lanczos (Real-ESRGAN sharpened their brush smears into
  streaks), and the planet bases are written opaque. At 4K the play plane
  shows roughly one texel per pixel near the screen's centre. The game
  samples terrain with trilinear mipmaps (`load_terrain` in main.rs):
  without them large textures, minified toward the planet's limb, alias
  into vertical bands and slow the frame.
- The normal maps are built from this terrain by tools/normalmaps.py's
  `normal_map`, at NORMAL_SIZE.

The game loads this set by default on desktop (MB_HD=1 the 4x set).
Usage:
  tools/upscale8x.py [--esrgan BINARY] [--only ui|sprites|font|textures|normals|all]
"""

import argparse
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "assets" / "4x"
OUT = ROOT / "assets" / "8x"
FFMPEG = shutil.which("ffmpeg")

# The normals are blurred relief, so 2048 holds all their detail at a quarter
# of the memory of the 4096 terrain.
NORMAL_SIZE = 2048

sys.path.insert(0, str(Path(__file__).resolve().parent))
import normalmaps  # noqa: E402
from upscale4x import DEFAULT_ESRGAN, edge_diff, terrain  # noqa: E402

def xbr_scale(src: Path, dst: Path) -> None:
    """xBR 2x, preserving alpha by scaling it as a separate mask."""
    im = Image.open(src)
    # oxipng stores many of these as palette or grey+alpha PNGs; their
    # transparency must take the alpha path too.
    if im.mode in ("LA", "PA") or "transparency" in im.info:
        im = im.convert("RGBA")
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


def upscale_textures(esrgan: Path) -> None:
    """8x terrain (4096x4096) from the originals; see upscale4x's `terrain`."""
    if not esrgan.exists():
        sys.exit(f"Real-ESRGAN not found at {esrgan} (pass --esrgan)")
    for p in sorted((ROOT / "original" / "textures").glob("*.png")):
        dst = OUT / "textures" / p.name
        dst.parent.mkdir(parents=True, exist_ok=True)
        terrain(p, esrgan, 8).save(dst, optimize=True)
        print("texture", p.name, "->", Image.open(dst).size, "edge", round(edge_diff(dst), 2))


def build_normals() -> None:
    for name in normalmaps.BASES:
        strength = normalmaps.STRENGTH.get(name, normalmaps.DEFAULT_STRENGTH)
        dst = OUT / "normals" / f"{name}.png"
        dst.parent.mkdir(parents=True, exist_ok=True)
        normalmaps.normal_map(OUT / "textures" / f"{name}.png", strength, NORMAL_SIZE).save(dst, optimize=True)
        print("normal", name, Image.open(dst).size)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--esrgan", type=Path, default=DEFAULT_ESRGAN)
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
        upscale_textures(args.esrgan)
    if args.only in ("normals", "all"):
        build_normals()


if __name__ == "__main__":
    main()

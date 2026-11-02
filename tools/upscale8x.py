#!/usr/bin/env python3
"""Build the experimental 8x art set from the committed 4x set.

- UI, sprites and the bitmap font: xBR 2x on the 4x pixel art, so the
  result is 8x the original. The font's glyph rectangles are scaled by 8.
- Terrain and fog: Real-ESRGAN x4plus at its native 4x (2048x2048 from the
  512x512 originals), upscaled with wrapped padding and cropped so they
  tile without any blending (`upscale_textures`); `close_seam` evens out
  the step some originals have at their wrap, the fogs use Lanczos, and the
  planet bases are written opaque. At 4K the 2x terrain is
  stretched about 1.9x near the screen's centre and looked soft beside the
  8x sprites. The game samples this set's terrain with trilinear mipmaps
  (`load_terrain` in main.rs): without them the 2048 textures, minified
  toward the planet's limb, aliased into vertical bands and slowed the frame.
- The normal maps are built from this terrain by tools/normalmaps.py's
  `normal_map`, at 2048x2048.

The game loads this set with MB_HD=8.
Usage:
  tools/upscale8x.py [--esrgan BINARY] [--only ui|sprites|font|textures|normals|all]
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
import normalmaps  # noqa: E402
from upscale4x import DEFAULT_ESRGAN, edge_diff, esrgan_upscale, tiled3  # noqa: E402

# Wrapped context around each terrain texture before upscaling, in original
# pixels; more than Real-ESRGAN's reach, so the cropped centre tiles exactly.
PAD = 128
# close_seam: the band the wrap's step is spread over, and the smoothing of
# the step along the edge, in pixels of the 2048 texture.
SEAM_BAND = 128
SEAM_SMOOTH = 32
# Upscaled with Lanczos, not Real-ESRGAN (see upscale_textures).
SOFT = {"fogBlackRed", "fogGreen", "fogRed"}


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


def upscale_textures(esrgan: Path) -> None:
    """4x terrain from the originals that tiles without blending.

    Real-ESRGAN only looks at a pixel's neighbourhood, so upscaling the
    original wrapped by PAD pixels on every side and cropping the centre
    gives an output whose edges continue into each other: the wrap is as
    smooth as any other step, and the detail is the same everywhere. The
    4x set's crossfade with a rolled copy instead left the outer ~256 pixels
    (at this size) with 56-70% of the centre's detail, a soft grid at every
    tile boundary.
    """
    if not esrgan.exists():
        sys.exit(f"Real-ESRGAN not found at {esrgan} (pass --esrgan)")
    for p in sorted((ROOT / "original" / "textures").glob("*.png")):
        dst = OUT / "textures" / p.name
        dst.parent.mkdir(parents=True, exist_ok=True)
        im = Image.open(p).convert("RGBA")
        # The clouds and fogs are decals: their alpha goes through the same
        # upscale as a grey image.
        # The fogs are soft overlays with faint brush smears near their
        # edges, which Real-ESRGAN sharpens into hard streaks; they get a
        # periodic Lanczos upscale instead.
        engine = None if p.stem in SOFT else esrgan
        rgb = periodic_upscale(im.convert("RGB"), engine)
        if p.stem in normalmaps.BASES:
            # The planet bases are opaque, but craters (and a few fire pixels)
            # were exported with alpha down to 216 on their 1-2 px border.
            # The backdrop multiplies by base alpha, so at 4x that border drew
            # a dark cross at every wrap; the bases are made fully opaque.
            alpha = Image.new("L", rgb.size, 255)
        else:
            alpha = periodic_upscale(im.getchannel("A").convert("RGB"), engine).convert("L")
        a = np.dstack([np.asarray(rgb, dtype=np.float64), np.asarray(alpha, dtype=np.float64)])
        a = close_seam(close_seam(a, 1), 0)
        Image.fromarray(np.clip(np.round(a), 0, 255).astype(np.uint8), "RGBA").save(dst, optimize=True)
        print("texture", p.name, im.size, "->", Image.open(dst).size, "edge", round(edge_diff(dst), 2))


def close_seam(a: np.ndarray, axis: int) -> np.ndarray:
    """Remove the step where the texture wraps on one axis.

    Some originals do not tile: the fogs jump by 1.5-1.8 times a typical
    step at their edges. The jump, smoothed along the edge (SEAM_SMOOTH), is
    split between the two sides and faded out over SEAM_BAND pixels, so the
    edges meet without blending in other content or losing detail.
    """
    a = np.moveaxis(a, axis, 0).copy()
    n = a.shape[0]
    step = a[0] - a[-1]
    # Smooth along the edge with wrap-around, so only the mismatch moves.
    k = np.exp(-0.5 * (np.arange(-3 * SEAM_SMOOTH, 3 * SEAM_SMOOTH + 1) / SEAM_SMOOTH) ** 2)
    k /= k.sum()
    pad = len(k) // 2
    wrapped = np.concatenate([step[-pad:], step, step[:pad]], axis=0)
    step = np.apply_along_axis(lambda v: np.convolve(v, k, mode="valid"), 0, wrapped)
    t = np.arange(SEAM_BAND) / SEAM_BAND
    ramp = (0.5 + 0.5 * np.cos(np.pi * t))[:, None, None] * 0.5
    a[:SEAM_BAND] -= ramp * step
    a[n - SEAM_BAND:] += ramp[::-1] * step
    return np.moveaxis(a, 0, axis)


def periodic_upscale(im: Image.Image, esrgan: Path | None) -> Image.Image:
    """4x of a tiling RGB image, upscaled with PAD wrapped pixels around it
    (Real-ESRGAN, or Lanczos when `esrgan` is None)."""
    w, h = im.size
    padded = tiled3(im).crop((w - PAD, h - PAD, 2 * w + PAD, 2 * h + PAD))
    if esrgan is None:
        big = padded.resize((padded.width * 4, padded.height * 4), Image.LANCZOS)
    else:
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / "padded.png"
            detail = Path(tmp) / "detail.png"
            padded.save(src)
            esrgan_upscale(src, detail, esrgan, 4)
            big = Image.open(detail).convert("RGB")
            big.load()
    return big.crop((PAD * 4, PAD * 4, (PAD + w) * 4, (PAD + h) * 4))


def build_normals() -> None:
    for name in normalmaps.BASES:
        strength = normalmaps.STRENGTH.get(name, normalmaps.DEFAULT_STRENGTH)
        dst = OUT / "normals" / f"{name}.png"
        dst.parent.mkdir(parents=True, exist_ok=True)
        normalmaps.normal_map(OUT / "textures" / f"{name}.png", strength).save(dst, optimize=True)
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

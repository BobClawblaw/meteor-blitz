#!/usr/bin/env python3
"""Upscale the original graphics for the HD render path.

- Terrain/fog/space/horizon textures: Real-ESRGAN x4plus when the ncnn
  binary is available, otherwise Lanczos, at 2x. The ncnn tool is run with
  one tile (`-t 1024`): its automatic 400 px tiling left visible seams.
  The backdrop mesh wraps these textures, so the ESRGAN detail is faded into
  a periodic Lanczos base near the edges; otherwise the upscaled edges do not
  match and the repeats show as a checkerboard. A final adaptive edge
  crossfade (`tileable`) makes the texture's own wrap continuous; the base
  alone was only continuous across the tiled pattern, so the fog textures
  still showed a line at the texture edge. The backdrop mesh samples these
  minified across the screen and macroquad builds no mipmaps, so a 4x copy
  (2048x2048) stalls the GPU and drops the frame rate; 2x holds 60 Hz.
- UI art, sprites and the bitmap font: xBR 4x (pixel-art aware), with the
  alpha channel upscaled separately so transparency survives.

Outputs mirror the source layout under assets/4x/ and are regenerable.
Usage:
  tools/upscale4x.py [--esrgan BINARY] [--only textures|ui|sprites|font|all]
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
# realesrgan-ncnn-vulkan 20220424 (xinntao/Real-ESRGAN v0.2.5.0), with its
# models/ folder beside it.
DEFAULT_ESRGAN = Path.home() / "tools/realesrgan/realesrgan-ncnn-vulkan"


def xbr4(src: Path, dst: Path) -> None:
    """xBR 4x, preserving alpha by scaling it as a separate mask."""
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
         "-f", "png", "-t", "1024"],
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


def tiled3(im: Image.Image) -> Image.Image:
    """A 3x3 copy: resampling its centre makes the result periodic."""
    w, h = im.size
    tiled = Image.new(im.mode, (w * 3, h * 3))
    for dy in range(3):
        for dx in range(3):
            tiled.paste(im, (dx * w, dy * h))
    return tiled


# Wrapped context around each terrain texture before upscaling, in original
# pixels; more than Real-ESRGAN's reach, so the cropped centre tiles exactly.
PAD = 128
# The margin kept for the second Real-ESRGAN pass of an 8x texture, in
# original pixels (128 px of its 4x input).
PAD2 = 32
# Upscaled with Lanczos, not Real-ESRGAN: the fogs are soft overlays with
# faint brush smears near their edges, which Real-ESRGAN sharpens into hard
# streaks.
SOFT = {"fogBlackRed", "fogGreen", "fogRed"}
# The planet bases (tools/normalmaps.py's BASES) are opaque, but craters (and
# a few fire pixels) were exported with alpha down to 216 on their 1-2 px
# border. The backdrop multiplies by base alpha, so upscaled, that border drew
# a dark cross at every wrap; the bases are written fully opaque.
OPAQUE = {"blue", "fire", "green", "sprawl", "craters", "space4deep"}


def esrgan4(im: Image.Image, esrgan: Path) -> Image.Image:
    with tempfile.TemporaryDirectory() as tmp:
        src = Path(tmp) / "in.png"
        dst = Path(tmp) / "out.png"
        im.save(src)
        esrgan_upscale(src, dst, esrgan, 4)
        out = Image.open(dst).convert("RGB")
        out.load()
    return out


def periodic_upscale(im: Image.Image, esrgan: Path | None, scale: int) -> Image.Image:
    """A tiling RGB image at `scale` (2, 4 or 8) that still tiles.

    Real-ESRGAN only looks at a pixel's neighbourhood, so upscaling the
    original wrapped by PAD pixels on every side and cropping the centre
    gives an output whose edges continue into each other: the wrap is as
    smooth as any other step, and the detail is the same everywhere.
    realesrgan-x4plus only works at 4x (asked for 2x it returns a scrambled
    image): 2x is its 4x output brought down with Lanczos, and 8x is a
    second 4x pass (on a PAD2 margin, to keep the 16x image small) brought
    down to 8x, which drew crisper edges than Lanczos or the anime 2x model
    from the 4x pass. Lanczos alone is used when `esrgan` is None.
    """
    w, h = im.size
    big = tiled3(im).crop((w - PAD, h - PAD, 2 * w + PAD, 2 * h + PAD))
    margin = PAD
    if esrgan is not None:
        big = esrgan4(big, esrgan)
        if scale > 4:
            k = (PAD - PAD2) * 4
            big = esrgan4(big.crop((k, k, big.width - k, big.height - k)), esrgan)
            margin = PAD2
    big = big.resize(((w + 2 * margin) * scale, (h + 2 * margin) * scale), Image.LANCZOS)
    return big.crop((margin * scale, margin * scale, (margin + w) * scale, (margin + h) * scale))


def close_seam(a: np.ndarray, axis: int) -> np.ndarray:
    """Remove the step where the texture wraps on one axis.

    Some originals do not tile: the fogs jump by 1.5-1.8 times a typical
    step at their edges. The jump, smoothed along the edge (1/512 of the
    size; wider smoothing left the fogs' and clouds' wrap 1.4-1.5 times a
    typical step), is split between the two sides and faded out over 1/16 of the
    size, so the edges meet without blending in other content or losing
    detail. (The crossfade with a rolled copy this replaced left the outer
    1/8 of each texture with 56-70% of the centre's detail, a soft grid at
    every tile boundary.)
    """
    a = np.moveaxis(a, axis, 0).copy()
    n = a.shape[0]
    band, smooth = n // 16, n / 512
    step = a[0] - a[-1]
    # Smooth along the edge with wrap-around, so only the mismatch moves.
    k = np.exp(-0.5 * (np.arange(-3 * int(smooth), 3 * int(smooth) + 1) / smooth) ** 2)
    k /= k.sum()
    pad = len(k) // 2
    wrapped = np.concatenate([step[-pad:], step, step[:pad]], axis=0)
    step = np.apply_along_axis(lambda v: np.convolve(v, k, mode="valid"), 0, wrapped)
    t = np.arange(band) / band
    ramp = (0.5 + 0.5 * np.cos(np.pi * t))[:, None, None] * 0.5
    a[:band] -= ramp * step
    a[n - band:] += ramp[::-1] * step
    return np.moveaxis(a, 0, axis)


def terrain(src: Path, esrgan: Path, scale: int) -> Image.Image:
    """One backdrop texture at `scale`, tiling with full detail to its edges."""
    im = Image.open(src).convert("RGBA")
    engine = None if src.stem in SOFT else esrgan
    rgb = periodic_upscale(im.convert("RGB"), engine, scale)
    if src.stem in OPAQUE:
        alpha = Image.new("L", rgb.size, 255)
    else:
        # The clouds and fogs are decals: their alpha goes through the same
        # upscale as a grey image.
        alpha = periodic_upscale(im.getchannel("A").convert("RGB"), engine, scale).convert("L")
    a = np.dstack([np.asarray(rgb, dtype=np.float64), np.asarray(alpha, dtype=np.float64)])
    a = close_seam(close_seam(a, 1), 0)
    return Image.fromarray(np.clip(np.round(a), 0, 255).astype(np.uint8), "RGBA")


def edge_diff(dst: Path) -> float:
    a = np.asarray(Image.open(dst).convert("RGB"), dtype=np.float32)
    return float((np.abs(a[:, 0] - a[:, -1]).mean() + np.abs(a[0, :] - a[-1, :]).mean()) / 2.0)


def upscale_textures(esrgan: Path) -> None:
    # 2x: the 4x set's backdrop.
    if not esrgan.exists():
        sys.exit(f"Real-ESRGAN not found at {esrgan} (pass --esrgan)")
    for p in sorted((ROOT / "original" / "textures").glob("*.png")):
        dst = OUT / "textures" / p.name
        dst.parent.mkdir(parents=True, exist_ok=True)
        terrain(p, esrgan, 2).save(dst, optimize=True)
        print("texture", p.name, "->", dst.relative_to(ROOT),
              "diff", round(verify(p, dst), 2), "edge", round(edge_diff(dst), 2))


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

#!/usr/bin/env python3
"""Build the 8x art set (the desktop default) directly from the originals.

- Sprites (`original/sprites/`), UI (`assets/ui/`), and bitmap font (`ngage.png`):
  edge-bled, reflect-padded RGB upscaled with Real-ESRGAN (`realesrgan-x4plus`,
  single 4x pass downsampled/upsampled cleanly to exact 8x via Lanczos), with
  smooth Bicubic alpha (no neural alpha warping, double halos, or edge bars).
  Neutral monochrome art (chrome labels, white fonts) is desaturated after
  upscaling to eliminate RGB chroma noise, and pure-white alpha-masked sheets
  keep pure white RGB. `github.png` is rendered directly from SVG at 128 px.
- Terrain and fog: 4096x4096 from the 512x512 originals, through
  upscale4x's `terrain` (two-pass padded Real-ESRGAN x4plus with seam closure).
- Normal maps: built from the 8x terrain by tools/normalmaps.py's `normal_map`,
  at NORMAL_SIZE.

The game loads this set by default on desktop (MB_HD=1 the 4x set, MB_HD=0
the originals).
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
OUT = ROOT / "assets" / "8x"

# The normals are blurred relief, so 2048 holds all their detail at a quarter
# of the memory of the 4096 terrain.
NORMAL_SIZE = 2048
# Edge-replication padding (in input pixels) around images fed to Real-ESRGAN
# so border pixels have full context without edge halos.
PAD = 16
# Tiny sprites (e.g. 2x2 aim dot, 10x10 HUD counters) stay cleaner with smooth
# Lanczos resampling than a neural upscaler, whose padding can warp tiny icons.
TINY_MAX = 12

sys.path.insert(0, str(Path(__file__).resolve().parent))
import normalmaps  # noqa: E402
from upscale4x import DEFAULT_ESRGAN, edge_diff, terrain  # noqa: E402


def bleed_rgb(rgba: np.ndarray, iters: int = 16) -> np.ndarray:
    """Dilate visible RGB into transparent pixels so upscalers never bleed
    background black/white across alpha edges."""
    rgb = rgba[..., :3].astype(np.float32)
    valid = rgba[..., 3] > 8
    if valid.all() or not valid.any():
        return rgba[..., :3]
    out = rgb.copy()
    mask = valid.copy()
    for _ in range(iters):
        if mask.all():
            break
        acc = np.zeros_like(out)
        cnt = np.zeros(mask.shape, dtype=np.float32)
        for dy, dx in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (-1, 1), (1, -1), (1, 1)]:
            m_shift = np.roll(np.roll(mask, dy, axis=0), dx, axis=1)
            r_shift = np.roll(np.roll(out, dy, axis=0), dx, axis=1)
            if dy > 0:
                m_shift[:dy, :] = False
            elif dy < 0:
                m_shift[dy:, :] = False
            if dx > 0:
                m_shift[:, :dx] = False
            elif dx < 0:
                m_shift[:, dx:] = False
            w = 1.0 if (dy == 0 or dx == 0) else 0.70710678
            acc += r_shift * (m_shift[..., None] * w)
            cnt += m_shift * w
        fill = (~mask) & (cnt > 0)
        if not fill.any():
            break
        out[fill] = acc[fill] / cnt[fill, None]
        mask |= fill
    return np.clip(np.round(out), 0, 255).astype(np.uint8)


def pad_reflect(im: Image.Image, pad: int) -> Image.Image:
    """Reflect-pad RGB before Real-ESRGAN so border features (like planet cubes
    and bevelled icons) never stretch into artificial edge bars."""
    arr = np.asarray(im.convert("RGB"))
    if arr.shape[0] > pad and arr.shape[1] > pad:
        padded = np.pad(arr, ((pad, pad), (pad, pad), (0, 0)), mode="reflect")
    else:
        padded = np.pad(arr, ((pad, pad), (pad, pad), (0, 0)), mode="edge")
    return Image.fromarray(padded, "RGB")


def run_esrgan_dir(in_dir: Path, out_dir: Path, esrgan: Path, model: str) -> None:
    """Batch-upscale every PNG in `in_dir` by 4x using a single GPU load."""
    out_dir.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            str(esrgan),
            "-i", str(in_dir),
            "-o", str(out_dir),
            "-s", "4",
            "-m", str(esrgan.parent / "models"),
            "-n", model,
            "-f", "png",
            "-t", "1024",
            "-j", "2:4:2",
        ],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def clean_alpha_noise(alpha: np.ndarray) -> np.ndarray:
    """Drop faint <= 5 stray alpha speckles on opaque-core sprites without
    touching glow-only sprites."""
    opaque = (alpha > 200).sum()
    if opaque > 0.05 * alpha.size:
        out = alpha.copy()
        out[out <= 5] = 0
        return out
    return alpha


def soft_fraction(alpha: np.ndarray) -> float:
    """Fraction of visible pixels that are semi-transparent (high for feathered
    glows/shadows, low for crisp silhouettes)."""
    a = alpha.astype(np.float32) / 255.0
    vis = (a > 0.05).sum()
    if vis == 0:
        return 1.0
    return float(((a > 0.05) & (a < 0.92)).sum()) / float(vis)


def upscale_batch(
    files: list[Path],
    dst_dir: Path,
    esrgan: Path,
    model: str = "realesrgan-x4plus",
    label: str = "asset",
) -> None:
    """Batch-upscale a list of PNGs to 8x with reflect-padded Real-ESRGAN x4plus
    RGB and clean Bicubic alpha (zero neural alpha warping or edge bars)."""
    dst_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        rgb_in = tmp / "rgb_in"
        rgb_4x = tmp / "rgb_4x"
        rgb_in.mkdir()

        # name -> (src_path, (w, h), has_alpha, a_arr, is_mono, is_pure_white)
        meta: dict[str, tuple[Path, tuple[int, int], bool, np.ndarray | None, bool, bool]] = {}
        tiny_files: list[Path] = []

        for p in files:
            im = Image.open(p)
            has_alpha = im.mode in ("RGBA", "LA", "PA") or "transparency" in im.info
            im = im.convert("RGBA")
            w, h = im.size
            if min(w, h) <= 10 or max(w, h) <= TINY_MAX:
                tiny_files.append(p)
                continue
            arr = np.asarray(im)
            vis = arr[..., 3] > 10 if has_alpha else np.ones((h, w), dtype=bool)
            if vis.any():
                r = arr[vis, 0].astype(np.int32)
                g = arr[vis, 1].astype(np.int32)
                b = arr[vis, 2].astype(np.int32)
                max_chroma = int(max(np.abs(r - g).max(), np.abs(g - b).max(), np.abs(r - b).max()))
                is_mono = max_chroma <= 12
                is_pure_white = has_alpha and int(min(r.min(), g.min(), b.min())) >= 245
            else:
                is_mono = False
                is_pure_white = False

            a_arr = clean_alpha_noise(arr[..., 3]) if has_alpha else None
            if not is_pure_white:
                bled = Image.fromarray(bleed_rgb(arr, 16), "RGB")
                pad_reflect(bled, PAD).save(rgb_in / p.name)
            meta[p.name] = (p, (w, h), has_alpha, a_arr, is_mono, is_pure_white)

        esrgan_jobs = list(rgb_in.glob("*.png"))
        if esrgan_jobs:
            print(f"{label}: {model} on {len(esrgan_jobs)} RGB files...", flush=True)
            run_esrgan_dir(rgb_in, rgb_4x, esrgan, model)

        for name, (src_path, (w, h), has_alpha, a_arr, is_mono, is_pure_white) in sorted(meta.items()):
            if is_pure_white:
                rgb8 = Image.new("RGB", (w * 8, h * 8), (255, 255, 255))
            else:
                im4 = Image.open(rgb_4x / name).convert("RGB")
                rgb_crop = im4.crop((PAD * 4, PAD * 4, (PAD + w) * 4, (PAD + h) * 4))
                rgb8 = rgb_crop.resize((w * 8, h * 8), Image.LANCZOS)
                if is_mono:
                    lum = rgb8.convert("L")
                    rgb8 = Image.merge("RGB", (lum, lum, lum))

            if has_alpha and a_arr is not None:
                if is_pure_white:
                    # Pure-white packed font/caption sheets (ngage.png, captions.png)
                    # have 1 px row gaps; Bilinear + smoothstep keeps strokes crisp,
                    # and masking out source-empty rows prevents cross-row bleed.
                    a_lin = np.asarray(
                        Image.fromarray(a_arr, "L").resize((w * 8, h * 8), Image.BILINEAR),
                        dtype=np.float32,
                    ) / 255.0
                    t = np.clip((a_lin - 0.25) / (0.75 - 0.25), 0.0, 1.0)
                    a_crisp = t * t * (3.0 - 2.0 * t)
                    # Zero out any upscaled row/column whose entire 1x source row/column was empty.
                    empty_rows = np.where(a_arr.max(axis=1) == 0)[0]
                    for ry in empty_rows:
                        a_crisp[ry * 8:(ry + 1) * 8, :] = 0.0
                    a8 = Image.fromarray(np.clip(np.round(a_crisp * 255.0), 0, 255).astype(np.uint8), "L")
                else:
                    a8 = Image.fromarray(a_arr, "L").resize((w * 8, h * 8), Image.BICUBIC)
                out = rgb8.convert("RGBA")
                out.putalpha(a8)
            else:
                out = rgb8
            out.save(dst_dir / name, optimize=True)

        for p in tiny_files:
            im = Image.open(p).convert("RGBA")
            w, h = im.size
            arr = np.asarray(im)
            vis = arr[..., 3] > 10
            if vis.any() and int(arr[vis, :3].min()) >= 245:
                rgb8 = Image.new("RGB", (w * 8, h * 8), (255, 255, 255))
                a8 = Image.fromarray(arr[..., 3], "L").resize((w * 8, h * 8), Image.BICUBIC)
                out = rgb8.convert("RGBA")
                out.putalpha(a8)
            else:
                # Edge-bleed RGB before Lanczos so tiny sprites never get dark fringes.
                bled = Image.fromarray(bleed_rgb(arr, 8), "RGB").resize((w * 8, h * 8), Image.LANCZOS)
                a8 = Image.fromarray(arr[..., 3], "L").resize((w * 8, h * 8), Image.BICUBIC)
                out = bled.convert("RGBA")
                out.putalpha(a8)
            out.save(dst_dir / p.name, optimize=True)

        # Fidelity guard: compare every output downsampled back to 1x vs original.
        worst = []
        for p in files:
            orig = Image.open(p).convert("RGBA")
            res = Image.open(dst_dir / p.name).convert("RGBA")
            down = res.resize(orig.size, Image.BOX)
            s = np.asarray(orig, dtype=np.float32)
            d = np.asarray(down, dtype=np.float32)
            sp = s[..., :3] * (s[..., 3:4] / 255.0)
            dp = d[..., :3] * (d[..., 3:4] / 255.0)
            rgb_mae = float(np.abs(sp - dp).mean())
            a_mae = float(np.abs(s[..., 3] - d[..., 3]).mean())
            worst.append((rgb_mae + a_mae, rgb_mae, a_mae, p.name))
        worst.sort(reverse=True)
        top = ", ".join(f"{n}({r:.1f}/{a:.1f})" for _, r, a, n in worst[:3])
        print(f"{label}: wrote {len(files)} files to {dst_dir.relative_to(ROOT)} [max rgb/a MAE: {top}]", flush=True)


def upscale_ui(esrgan: Path) -> None:
    files = [p for p in sorted((ROOT / "assets" / "ui").glob("*.png")) if p.name != "github.png"]
    upscale_batch(files, OUT / "ui", esrgan, "realesrgan-x4plus", label="ui")
    # Render github.png directly from the SVG at 128x128 (8x of 16 px) if possible.
    rsvg = shutil.which("rsvg-convert")
    svg = ROOT / "tools" / "github-mark.svg"
    dst = OUT / "ui" / "github.png"
    if rsvg and svg.exists():
        subprocess.run([rsvg, "-w", "128", "-h", "128", "-o", str(dst), str(svg)], check=True)
        print("ui: rendered github.png from SVG (128x128)", flush=True)
    else:
        upscale_batch([ROOT / "assets" / "ui" / "github.png"], OUT / "ui", esrgan, "realesrgan-x4plus", label="ui-github")


def upscale_sprites(esrgan: Path) -> None:
    files = sorted((ROOT / "original" / "sprites").glob("*.png"))
    upscale_batch(files, OUT / "sprites", esrgan, "realesrgan-x4plus", label="sprites")


def upscale_font(esrgan: Path) -> None:
    src = ROOT / "assets" / "ui" / "ngage.png"
    upscale_batch([src], OUT, esrgan, "realesrgan-x4plus", label="font")
    dat = (ROOT / "original" / "app" / "ngage.dat").read_bytes()
    count = int.from_bytes(dat[0:2], "little")
    out = bytearray(dat[0:2])
    for i in range(count):
        for k in range(4):
            at = 2 + i * 8 + k * 2
            v = int.from_bytes(dat[at:at + 2], "little") * 8
            out += v.to_bytes(2, "little")
    (OUT / "ngage.dat").write_bytes(out)
    print("font: wrote ngage.dat (8x metrics)", flush=True)


def upscale_textures(esrgan: Path) -> None:
    """8x terrain (4096x4096) from the originals; see upscale4x's `terrain`."""
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
    ap.add_argument(
        "--only",
        choices=["ui", "sprites", "font", "textures", "normals", "all"],
        default="all",
    )
    args = ap.parse_args()
    if not args.esrgan.exists() and args.only != "normals":
        sys.exit(f"Real-ESRGAN not found at {args.esrgan} (pass --esrgan)")
    if args.only in ("ui", "all"):
        upscale_ui(args.esrgan)
    if args.only in ("sprites", "all"):
        upscale_sprites(args.esrgan)
    if args.only in ("font", "all"):
        upscale_font(args.esrgan)
    if args.only in ("textures", "all"):
        upscale_textures(args.esrgan)
    if args.only in ("normals", "all"):
        build_normals()


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Build the 8x art set (the desktop default) directly from the originals.

- Sprites (`original/sprites/`): edge-bled RGB upscaled with two-pass
  Real-ESRGAN (`realesrgan-x4plus`) downsampled to exact 8x via Lanczos, plus
  hybrid alpha upscaling (`realesrgan-x4plus-anime` for hard silhouettes blended
  with smooth bicubic for soft glows/shadows). Replaces the previous xBR 4x+2x
  chain, which caused onion-ring alpha banding and painterly smearing on the
  game's pre-rendered 3D sprites.
- UI (`assets/ui/`) and bitmap font (`ngage.png`): edge-bled RGB upscaled with
  `realesrgan-x4plus-anime` (sharp chrome text, bevels, and silhouettes) brought
  to 8x via Lanczos, with hybrid alpha (and `rsvg-convert` at 128 px for
  `github.png` when available). The font's glyph rectangles are scaled by 8.
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


def pad_edge(im: Image.Image, pad: int) -> Image.Image:
    arr = np.asarray(im.convert("RGB"))
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
    model: str,
    two_pass: bool,
    force_sharp_alpha: bool = False,
    label: str = "asset",
) -> None:
    """Batch-upscale a list of PNGs to 8x with edge-bled RGB and clean alpha."""
    dst_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        rgb_in = tmp / "rgb_in"
        rgb_4x = tmp / "rgb_4x"
        rgb_p2_in = tmp / "rgb_p2_in"
        rgb_16x = tmp / "rgb_16x"
        alpha_in = tmp / "alpha_in"
        alpha_4x = tmp / "alpha_4x"
        rgb_in.mkdir()
        alpha_in.mkdir()

        meta: dict[str, tuple[Path, tuple[int, int], bool, np.ndarray | None, float]] = {}
        tiny_files: list[Path] = []

        for p in files:
            im = Image.open(p)
            has_alpha = im.mode in ("RGBA", "LA", "PA") or "transparency" in im.info
            im = im.convert("RGBA")
            w, h = im.size
            if max(w, h) <= TINY_MAX:
                tiny_files.append(p)
                continue
            arr = np.asarray(im)
            bled = Image.fromarray(bleed_rgb(arr, 16), "RGB")
            pad_edge(bled, PAD).save(rgb_in / p.name)
            a_arr = None
            sf = 0.0
            if has_alpha:
                a_arr = clean_alpha_noise(arr[..., 3])
                sf = soft_fraction(a_arr)
                if force_sharp_alpha or sf < 0.45:
                    pad_edge(Image.fromarray(a_arr, "L").convert("RGB"), PAD).save(alpha_in / p.name)
            meta[p.name] = (p, (w, h), has_alpha, a_arr, sf)

        if meta:
            print(f"{label}: pass 1 ({model}) on {len(meta)} RGB files...", flush=True)
            run_esrgan_dir(rgb_in, rgb_4x, esrgan, model)

            if two_pass:
                rgb_p2_in.mkdir()
                for name, (_, (w, h), _, _, _) in meta.items():
                    im4 = Image.open(rgb_4x / name).convert("RGB")
                    cropped = im4.crop((PAD * 4, PAD * 4, (PAD + w) * 4, (PAD + h) * 4))
                    pad_edge(cropped, PAD).save(rgb_p2_in / name)
                print(f"{label}: pass 2 ({model}) on {len(meta)} RGB files...", flush=True)
                run_esrgan_dir(rgb_p2_in, rgb_16x, esrgan, model)

            alpha_jobs = list(alpha_in.glob("*.png"))
            if alpha_jobs:
                print(f"{label}: alpha pass (realesrgan-x4plus-anime) on {len(alpha_jobs)} files...", flush=True)
                run_esrgan_dir(alpha_in, alpha_4x, esrgan, "realesrgan-x4plus-anime")

            for name, (src_path, (w, h), has_alpha, a_arr, sf) in sorted(meta.items()):
                if two_pass:
                    im16 = Image.open(rgb_16x / name).convert("RGB")
                    rgb_crop = im16.crop((PAD * 4, PAD * 4, (PAD + w * 4) * 4, (PAD + h * 4) * 4))
                else:
                    im4 = Image.open(rgb_4x / name).convert("RGB")
                    rgb_crop = im4.crop((PAD * 4, PAD * 4, (PAD + w) * 4, (PAD + h) * 4))
                rgb8 = rgb_crop.resize((w * 8, h * 8), Image.LANCZOS)

                if has_alpha and a_arr is not None:
                    a_soft = np.asarray(
                        Image.fromarray(a_arr, "L").resize((w * 8, h * 8), Image.BICUBIC),
                        dtype=np.float32,
                    )
                    sharp_path = alpha_4x / name
                    if sharp_path.exists():
                        a4 = Image.open(sharp_path).convert("L")
                        a4_crop = a4.crop((PAD * 4, PAD * 4, (PAD + w) * 4, (PAD + h) * 4))
                        a_sharp = np.asarray(a4_crop.resize((w * 8, h * 8), Image.BICUBIC), dtype=np.float32)
                        if force_sharp_alpha:
                            a_mix = a_sharp
                        else:
                            t = float(np.clip((sf - 0.15) / 0.30, 0.0, 1.0))
                            a_mix = a_sharp * (1.0 - t) + a_soft * t
                    else:
                        a_mix = a_soft
                    a8 = Image.fromarray(np.clip(np.round(a_mix), 0, 255).astype(np.uint8), "L")
                    out = rgb8.convert("RGBA")
                    out.putalpha(a8)
                else:
                    out = rgb8
                out.save(dst_dir / name, optimize=True)

        for p in tiny_files:
            im = Image.open(p).convert("RGBA")
            w, h = im.size
            out = im.resize((w * 8, h * 8), Image.LANCZOS)
            out.save(dst_dir / p.name, optimize=True)

        print(f"{label}: wrote {len(files)} files to {dst_dir.relative_to(ROOT)}", flush=True)


def upscale_ui(esrgan: Path) -> None:
    files = [p for p in sorted((ROOT / "assets" / "ui").glob("*.png")) if p.name != "github.png"]
    upscale_batch(files, OUT / "ui", esrgan, "realesrgan-x4plus-anime", two_pass=False, label="ui")
    # Render github.png directly from the SVG at 128x128 (8x of 16 px) if possible.
    rsvg = shutil.which("rsvg-convert")
    svg = ROOT / "tools" / "github-mark.svg"
    dst = OUT / "ui" / "github.png"
    if rsvg and svg.exists():
        subprocess.run([rsvg, "-w", "128", "-h", "128", "-o", str(dst), str(svg)], check=True)
        print("ui: rendered github.png from SVG (128x128)", flush=True)
    else:
        upscale_batch([ROOT / "assets" / "ui" / "github.png"], OUT / "ui", esrgan, "realesrgan-x4plus-anime", two_pass=False, label="ui-github")


def upscale_sprites(esrgan: Path) -> None:
    files = sorted((ROOT / "original" / "sprites").glob("*.png"))
    upscale_batch(files, OUT / "sprites", esrgan, "realesrgan-x4plus", two_pass=True, label="sprites")


def upscale_font(esrgan: Path) -> None:
    src = ROOT / "assets" / "ui" / "ngage.png"
    upscale_batch(
        [src],
        OUT,
        esrgan,
        "realesrgan-x4plus-anime",
        two_pass=False,
        force_sharp_alpha=True,
        label="font",
    )
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

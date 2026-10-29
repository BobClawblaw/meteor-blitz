#!/usr/bin/env python3
"""App icons for the release builds, from the original 57x57 iOS icon.

Upscales assets/ui/icon.57x57.png (decoded by tools/decode_ui.py) 4x with
Real-ESRGAN when it is available (Lanczos otherwise), then writes into
assets/icon/:

  icon-1024.png        the master
  icon-16/32/64.png    window icons (src/main.rs embeds them)
  icon.ico             Windows executable icon (16-256)
  icon.icns            macOS app icon (needs `iconutil`, macOS only)

  tools/make-icon.py [--esrgan PATH_TO/realesrgan-ncnn-vulkan]
"""

import argparse
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "assets/ui/icon.57x57.png"
OUT = ROOT / "assets/icon"


def upscale(esrgan: Path | None) -> Image.Image:
    """The icon at 228x228: Real-ESRGAN 4x if it runs, else Lanczos."""
    if esrgan and esrgan.exists():
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "icon.png"
            subprocess.run(
                [str(esrgan), "-i", str(SRC), "-o", str(out), "-m", str(esrgan.parent / "models"),
                 "-n", "realesrgan-x4plus", "-s", "4"],
                check=False, capture_output=True,
            )
            if out.exists():
                return Image.open(out).convert("RGBA")
    print("Real-ESRGAN unavailable: using Lanczos", file=sys.stderr)
    return Image.open(SRC).convert("RGBA").resize((228, 228), Image.LANCZOS)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--esrgan", type=Path, default=Path("/tmp/resrgan/realesrgan-ncnn-vulkan"))
    args = ap.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    master = upscale(args.esrgan).resize((1024, 1024), Image.LANCZOS)
    master.save(OUT / "icon-1024.png", optimize=True)
    for size in (16, 32, 64):
        master.resize((size, size), Image.LANCZOS).save(OUT / f"icon-{size}.png", optimize=True)
    master.save(OUT / "icon.ico", sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)])
    if shutil.which("iconutil"):
        with tempfile.TemporaryDirectory() as tmp:
            iconset = Path(tmp) / "icon.iconset"
            iconset.mkdir()
            for size in (16, 32, 128, 256, 512):
                master.resize((size, size), Image.LANCZOS).save(iconset / f"icon_{size}x{size}.png")
                master.resize((size * 2, size * 2), Image.LANCZOS).save(iconset / f"icon_{size}x{size}@2x.png")
            subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(OUT / "icon.icns")], check=True)
    for f in sorted(OUT.iterdir()):
        print(f.relative_to(ROOT))


if __name__ == "__main__":
    main()

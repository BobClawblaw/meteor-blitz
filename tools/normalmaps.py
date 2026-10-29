#!/usr/bin/env python3
"""Normal maps for the planet textures (port addition: lit terrain).

Treats each base texture's brightness as height, smooths it, and takes the
gradient with wrap-around (the backdrop shader tiles these textures), so the
normals tile too. Writes RGB normals (x right, y up in texture v, z out) at
the original size into assets/normals/ and at the HD size into
assets/4x/normals/:

  tools/normalmaps.py
"""

from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
BASES = ["blue", "fire", "green", "sprawl", "craters", "space4deep"]
# How strongly brightness reads as relief; the starfield stays nearly flat.
STRENGTH = {"space4deep": 0.6}
DEFAULT_STRENGTH = 3.0


def blur(h: np.ndarray, passes: int) -> np.ndarray:
    """A small wrap-around box blur, repeated."""
    for _ in range(passes):
        h = (h + np.roll(h, 1, 0) + np.roll(h, -1, 0) + np.roll(h, 1, 1) + np.roll(h, -1, 1)) / 5.0
    return h


def normal_map(src: Path, strength: float) -> Image.Image:
    rgb = np.asarray(Image.open(src).convert("RGB"), dtype=np.float32) / 255.0
    height = rgb @ np.array([0.299, 0.587, 0.114], dtype=np.float32)
    # Dark and bright worlds alike: relief from the contrast, not the level.
    height = (height - height.mean()) / max(float(height.std()), 1e-3) * 0.12
    height = blur(height, max(1, rgb.shape[0] // 256))
    # Central differences with wrap-around; image rows run down, texture v up.
    dx = (np.roll(height, -1, 1) - np.roll(height, 1, 1)) * 0.5
    dy = (np.roll(height, 1, 0) - np.roll(height, -1, 0)) * 0.5
    scale = strength * rgb.shape[0] / 512.0
    n = np.stack([-dx * scale, -dy * scale, np.ones_like(height)], axis=-1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    return Image.fromarray(((n * 0.5 + 0.5) * 255.0 + 0.5).astype(np.uint8), "RGB")


def main() -> None:
    for name in BASES:
        strength = STRENGTH.get(name, DEFAULT_STRENGTH)
        for src_dir, out_dir in [
            (ROOT / "original/textures", ROOT / "assets/normals"),
            (ROOT / "assets/4x/textures", ROOT / "assets/4x/normals"),
        ]:
            out_dir.mkdir(parents=True, exist_ok=True)
            out = out_dir / f"{name}.png"
            normal_map(src_dir / f"{name}.png", strength).save(out, optimize=True)
            print(out.relative_to(ROOT))


if __name__ == "__main__":
    main()

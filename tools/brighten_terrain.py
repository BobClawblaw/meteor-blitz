#!/usr/bin/env python3
"""Brighten the original planet textures for the port's lighting.

The 2009 assets are very dark; the port needs readable terrain. This is a
derived asset step: original/ stays untouched, assets/terrain/ is regenerated.
"""

import os
from PIL import Image, ImageEnhance

ROOT = os.path.join(os.path.dirname(__file__), "..")
SRC = os.path.join(ROOT, "original", "textures")
DST = os.path.join(ROOT, "assets", "terrain")

def main():
    os.makedirs(DST, exist_ok=True)
    for name in os.listdir(SRC):
        if not name.endswith(".png"):
            continue
        im = Image.open(os.path.join(SRC, name)).convert("RGB")
        im = ImageEnhance.Brightness(im).enhance(2.6)
        im = ImageEnhance.Contrast(im).enhance(1.15)
        im.save(os.path.join(DST, name))
        print("brightened", name)

if __name__ == "__main__":
    main()

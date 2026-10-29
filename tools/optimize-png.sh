#!/bin/bash
# Lossless PNG recompression (oxipng; `brew install oxipng`) of the derived
# art in assets/: same pixels, ~10% fewer bytes. Run it last, after any of
# the asset scripts (decode_ui.py, upscale4x.py, normalmaps.py, make-icon.py,
# make-github-mark.py), which write unoptimised PNGs. original/ is never
# touched; tools/web_pack.py optimises copies of what it packs from there.
set -euo pipefail
cd "$(dirname "$0")/.."
oxipng -o 4 --strip safe -r -q assets/ui assets/4x assets/normals assets/icon
du -sh assets/ui assets/4x assets/normals assets/icon

#!/usr/bin/env python3
"""The GitHub mark for the main menu's project link (port addition).

Renders tools/github-mark.svg (the octicon `mark-github`, white) with
rsvg-convert (librsvg; `brew install librsvg`) to assets/ui/github.png
(16 px, the original art set) and assets/4x/ui/github.png (64 px, HD):

  tools/make-github-mark.py
"""

import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SVG = ROOT / "tools/github-mark.svg"


def main() -> None:
    for size, out in [(16, ROOT / "assets/ui/github.png"), (64, ROOT / "assets/4x/ui/github.png")]:
        subprocess.run(
            ["rsvg-convert", "-w", str(size), "-h", str(size), "-o", str(out), str(SVG)],
            check=True,
        )
        print(out.relative_to(ROOT))


if __name__ == "__main__":
    main()

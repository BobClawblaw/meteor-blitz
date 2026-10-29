#!/usr/bin/env python3
"""The web build's data pack: every file the game loads at run time, in one
gzipped file the page downloads in a single request (web/data.pack.gz).

Loading the files one by one took minutes over the internet (hundreds of
round trips before the first frame); index.html fetches this pack with a
progress bar and serves the game's file requests from it.

Format (little endian): b"MBPK1", u32 count, then per file: u16 path length,
the UTF-8 path (as the game requests it, e.g. "assets/ui/buttonPlay.png"),
u32 size, the bytes. Entries are sorted and the gzip header has no
timestamp, so an unchanged data set rebuilds byte for byte.

  tools/web_pack.py
"""

import gzip
import hashlib
import shutil
import struct
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "web/data.pack.gz"

# What the web build reads (it loads the original-resolution art, not
# assets/4x; the font and icons are built into the binary).
DIRS = ["assets/ui", "assets/normals", "assets/web-audio", "original/sprites", "original/textures"]
FILES = ["original/app/ngage.dat", "original/app/systems.dat"]
# original/ is read-only: its PNGs are packed as losslessly recompressed
# copies (oxipng, same pixels), cached by content hash between builds.
CACHE = ROOT / "target/web-pack-png"


def png_bytes(path: Path) -> bytes:
    data = path.read_bytes()
    if not path.as_posix().startswith(str(ROOT / "original")) or path.suffix != ".png":
        return data
    if shutil.which("oxipng") is None:
        return data
    CACHE.mkdir(parents=True, exist_ok=True)
    cached = CACHE / (hashlib.sha256(data).hexdigest() + ".png")
    if not cached.exists():
        tmp = cached.with_suffix(".tmp.png")
        tmp.write_bytes(data)
        subprocess.run(["oxipng", "-o", "4", "--strip", "safe", "-q", str(tmp)], check=True)
        tmp.rename(cached)
    return cached.read_bytes()


def main() -> None:
    paths = sorted(
        {p for d in DIRS for p in (ROOT / d).rglob("*") if p.is_file() and not p.name.startswith(".")}
        | {ROOT / f for f in FILES}
    )
    body = bytearray(b"MBPK1" + struct.pack("<I", len(paths)))
    for p in paths:
        rel = p.relative_to(ROOT).as_posix().encode()
        data = png_bytes(p)
        body += struct.pack("<H", len(rel)) + rel + struct.pack("<I", len(data)) + data
    with open(OUT, "wb") as raw, gzip.GzipFile(fileobj=raw, mode="wb", compresslevel=9, mtime=0, filename="") as gz:
        gz.write(body)
    print(f"{OUT.relative_to(ROOT)}: {len(paths)} files, {len(body):,} bytes -> {OUT.stat().st_size:,} gzipped")


if __name__ == "__main__":
    main()

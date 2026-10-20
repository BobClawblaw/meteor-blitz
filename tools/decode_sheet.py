#!/usr/bin/env python3
"""Unpack original/app/game.raw, the packed texture sheet, into named PNGs.

The file is a stream of texture records in the order GameScreen::loadTextures
reads them (`Texture::load(DataBuffer*)` in the iOS binary):

  compressed: int w, int h, uint glFormat (0x8C03 = PVRTC 2bpp, 0x8C02 = 4bpp),
              uint byteSize, then the PVRTC data
  raw:        int w, int h, uint texW, uint texH, float uScale, float vScale,
              then texW*texH pixels as RGBA4444 (2 bytes) or RGBA8888 (4 bytes)

Records are occasionally separated by a few zero bytes, which are skipped.
Names come from docs/sheet-names.txt (the load order recovered from the
binary). Output: original/sprites/<name>.png cropped to w x h, and
docs/sheet-index.txt with every record's geometry.

Usage: tools/decode_sheet.py [--out DIR]
"""

import argparse
import struct
import subprocess
import sys
from pathlib import Path

import numpy as np
from PIL import Image

from decode_textures import build

ROOT = Path(__file__).resolve().parent.parent
SHEET = ROOT / "original" / "app" / "game.raw"
NAMES = ROOT / "docs" / "sheet-names.txt"
PVRTC_2BPP = 0x8C03
PVRTC_4BPP = 0x8C02
MAX_DIM = 1024


def plausible_header(data: bytes, off: int) -> bool:
    """True when a texture record (or the end of file) starts at `off`."""
    if off >= len(data):
        return True
    if off + 16 > len(data):
        return False
    w, h, f3, f4 = struct.unpack_from("<4I", data, off)
    if w == 0:
        return True  # zero padding before the next record
    if f3 in (PVRTC_2BPP, PVRTC_4BPP):
        return 1 <= w <= MAX_DIM and 1 <= h <= MAX_DIM and f4 == w * h // (4 if f3 == PVRTC_2BPP else 2)
    return 1 <= w <= f3 <= MAX_DIM and 1 <= h <= f4 <= MAX_DIM


def parse(data: bytes, formats):
    """Yields records; `formats[i]` is the i-th record's pixel format from the
    load call (1 = RGBA8888, 2 = RGBA4444), or None to infer it."""
    off = 0
    index = 0
    while off + 16 <= len(data):
        w, h, f3, f4 = struct.unpack_from("<4I", data, off)
        if w == 0:
            off += 4  # zero padding between records; a record never has w == 0
            continue
        if f3 in (PVRTC_2BPP, PVRTC_4BPP):
            yield dict(off=off, kind="pvrtc", w=w, h=h, tw=w, th=h, bpp=2 if f3 == PVRTC_2BPP else 4, data=data[off + 16:off + 16 + f4])
            off += 16 + f4
            index += 1
            continue
        tw, th = f3, f4
        if not (1 <= w <= tw <= MAX_DIM and 1 <= h <= th <= MAX_DIM):
            sys.exit(f"cannot parse record at offset {off}: {(w, h, f3, f4)}")
        u_scale, v_scale = struct.unpack_from("<2f", data, off + 16)
        known = formats[index] if index < len(formats) else None
        candidates = {1: [4], 2: [2]}.get(known, [2, 4])
        for bytes_px in candidates:
            size = tw * th * bytes_px
            if off + 24 + size <= len(data) and plausible_header(data, off + 24 + size):
                yield dict(off=off, kind=f"raw{bytes_px * 8}", w=w, h=h, tw=tw, th=th, u=u_scale, v=v_scale, data=data[off + 24:off + 24 + size])
                off += 24 + size
                index += 1
                break
        else:
            sys.exit(f"cannot parse record {index} at offset {off}: {(w, h, f3, f4)}")


def decode(rec, exe: Path, tmp: Path) -> Image.Image:
    if rec["kind"] == "pvrtc":
        (tmp / "t.bin").write_bytes(rec["data"])
        subprocess.run([str(exe), "--raw", str(rec["tw"]), str(rec["th"]), str(rec["bpp"]), "0", str(tmp / "t.bin"), str(tmp / "t.rgba")], check=True, capture_output=True)
        im = Image.frombytes("RGBA", (rec["tw"], rec["th"]), (tmp / "t.rgba").read_bytes())
    elif rec["kind"] == "raw16":
        a = np.frombuffer(rec["data"], dtype="<u2").reshape(rec["th"], rec["tw"])
        rgba = np.stack([((a >> 12) & 15) * 17, ((a >> 8) & 15) * 17, ((a >> 4) & 15) * 17, (a & 15) * 17], axis=-1)
        im = Image.fromarray(rgba.astype("uint8"), "RGBA")
    else:
        a = np.frombuffer(rec["data"], dtype="uint8").reshape(rec["th"], rec["tw"], 4)
        im = Image.fromarray(a.copy(), "RGBA")
    return im.crop((0, 0, rec["w"], rec["h"]))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pvrt-dir", default=str(Path.home() / "tools/touchHLE-src/vendor/PVRTDecompress"))
    ap.add_argument("--out", default=str(ROOT / "original" / "sprites"))
    ap.add_argument("--build-dir", default=str(ROOT / "target" / "pvr_decode"))
    args = ap.parse_args()
    exe = build(Path(args.pvrt_dir), Path(args.build_dir))
    tmp = Path(args.build_dir)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob("*.png"):
        old.unlink()
    rows = [line.split(" ", 2) for line in NAMES.read_text().splitlines() if line and not line.startswith("#")]
    names = [row[2].strip() for row in rows]
    formats = [int(row[1]) if row[1].isdigit() else None for row in rows]
    records = list(parse(SHEET.read_bytes(), formats))
    if len(records) != len(names):
        print(f"warning: {len(records)} records but {len(names)} names; extra records keep numeric names")
    with open(ROOT / "docs" / "sheet-index.txt", "w") as index:
        index.write("# index name kind w h texW texH uScale vScale (from original/app/game.raw)\n")
        for i, rec in enumerate(records):
            name = names[i] if i < len(names) else f"record{i:03}"
            stem = name.replace(".2bits.pvr", "").replace(".png", "")
            decode(rec, exe, tmp).save(out / f"{stem}.png")
            index.write(f"{i} {stem} {rec['kind']} {rec['w']} {rec['h']} {rec['tw']} {rec['th']} {rec.get('u', 1.0):.4f} {rec.get('v', 1.0):.4f}\n")
    print(f"decoded {len(records)} textures into {out}")


if __name__ == "__main__":
    main()

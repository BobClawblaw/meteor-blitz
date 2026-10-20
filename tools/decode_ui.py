#!/usr/bin/env python3
"""Decode Apple CgBI ("crushed") PNGs from the iOS bundle into standard PNGs.

iOS build tools rewrite PNGs: a CgBI chunk is prepended, IDAT is raw deflate
(no zlib header), pixels are BGRA, and colour is premultiplied by alpha.
This undoes all three and writes ordinary RGBA PNGs.

Usage: tools/decode_ui.py [src_dir] [dst_dir]
Defaults: original/app -> assets/ui
"""

import struct
import sys
import zlib
from pathlib import Path

from PIL import Image

PNG_SIG = b"\x89PNG\r\n\x1a\n"


def chunks(data):
    pos = 8
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos:pos + 4])
        kind = data[pos + 4:pos + 8]
        yield kind, data[pos + 8:pos + 8 + length]
        pos += 12 + length


def paeth(a, b, c):
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    return b if pb <= pc else c


def unfilter(raw, width, height, bpp):
    stride = width * bpp
    out = bytearray(stride * height)
    prev = bytearray(stride)
    pos = 0
    for y in range(height):
        ftype = raw[pos]
        line = bytearray(raw[pos + 1:pos + 1 + stride])
        pos += 1 + stride
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if ftype == 1:
                line[i] = (line[i] + a) & 0xFF
            elif ftype == 2:
                line[i] = (line[i] + b) & 0xFF
            elif ftype == 3:
                line[i] = (line[i] + ((a + b) >> 1)) & 0xFF
            elif ftype == 4:
                line[i] = (line[i] + paeth(a, b, c)) & 0xFF
        out[y * stride:(y + 1) * stride] = line
        prev = line
    return bytes(out)


def decode(path):
    data = path.read_bytes()
    if data[:8] != PNG_SIG:
        raise ValueError("not a PNG")
    found = dict()
    idat = bytearray()
    for kind, body in chunks(data):
        if kind == b"IDAT":
            idat += body
        elif kind not in found:
            found[kind] = body
    if b"CgBI" not in found:
        return Image.open(path).convert("RGBA")
    width, height, depth, ctype = struct.unpack(">IIBB", found[b"IHDR"][:10])
    if depth != 8 or ctype not in (2, 6):
        raise ValueError(f"unsupported depth={depth} colour type={ctype}")
    bpp = 4 if ctype == 6 else 3
    pixels = unfilter(zlib.decompress(bytes(idat), -15), width, height, bpp)
    mode = "RGBA" if bpp == 4 else "RGB"
    # Stored order is BGR(A); reorder channels.
    im = Image.frombytes(mode, (width, height), pixels)
    chans = im.split()
    if bpp == 4:
        b, g, r, a = chans
        im = Image.merge("RGBA", (r, g, b, a))
        # Undo premultiplied alpha.
        px = bytearray(im.tobytes())
        for i in range(0, len(px), 4):
            alpha = px[i + 3]
            if 0 < alpha < 255:
                for k in range(3):
                    px[i + k] = min(255, px[i + k] * 255 // alpha)
        im = Image.frombytes("RGBA", (width, height), bytes(px))
    else:
        b, g, r = chans
        im = Image.merge("RGB", (r, g, b)).convert("RGBA")
    return im


def main():
    src = Path(sys.argv[1] if len(sys.argv) > 1 else "original/app")
    dst = Path(sys.argv[2] if len(sys.argv) > 2 else "assets/ui")
    dst.mkdir(parents=True, exist_ok=True)
    ok = 0
    for path in sorted(src.glob("*.png")):
        try:
            decode(path).save(dst / path.name)
            ok += 1
        except Exception as err:  # report and keep going
            print(f"skip {path.name}: {err}")
    print(f"decoded {ok} images into {dst}")


if __name__ == "__main__":
    main()

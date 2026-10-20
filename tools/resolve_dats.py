#!/usr/bin/env python3
"""Resolve the DAT_xxxxxxxx literals in a Ghidra-decompiled function.

For each DAT_ reference in the given .c file(s), prints the word at that
address and what it looks like: a float, a CFString ("..."), a pointer to a
named global (with the global's float/int value), or a plain integer.

Usage: tools/resolve_dats.py <decompiled.c> [more.c ...]
Needs original/app/Meteor Blitz and `nm -n` output (cached in target/syms.txt).
"""

import re
import struct
import subprocess
import sys
from bisect import bisect_right
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "original" / "app" / "Meteor Blitz"
SYMS = ROOT / "target" / "syms.txt"
VM_TO_FILE = 0x1000  # __TEXT and __DATA both map file offset = address - 0x1000
CFSTRING = (0x9475C, 0x9475C + 0x28C0)
CSTRING = (0x898FC, 0x898FC + 0x611D)
SELREFS = (0x97764, 0x97DF8)
MAX_ADDR = 0xB0000
MIN_ADDR = 0x2DEC  # start of __text: smaller words are plain integers (state ids etc.)


def load_syms():
    if not SYMS.exists():
        SYMS.parent.mkdir(exist_ok=True)
        SYMS.write_text(subprocess.run(["nm", "-n", str(BIN)], capture_output=True, text=True, check=True).stdout)
    addrs, names = [], []
    for line in SYMS.read_text().splitlines():
        parts = line.split()
        if len(parts) == 3:
            addrs.append(int(parts[0], 16))
            names.append(parts[2])
    return addrs, names


class Binary:
    def __init__(self):
        self.data = BIN.read_bytes()
        self.addrs, self.names = load_syms()

    def word(self, addr):
        off = addr - VM_TO_FILE
        return struct.unpack_from("<I", self.data, off)[0]

    def float_(self, addr):
        return struct.unpack_from("<f", self.data, addr - VM_TO_FILE)[0]

    def cstring(self, addr):
        off = addr - VM_TO_FILE
        end = self.data.index(b"\0", off)
        return self.data[off:end].decode(errors="replace")

    def symbol(self, addr):
        i = bisect_right(self.addrs, addr) - 1
        if i >= 0 and addr - self.addrs[i] < 0x40:
            name = self.names[i]
            delta = addr - self.addrs[i]
            return name if delta == 0 else f"{name}+{delta:#x}"
        return None

    def describe(self, addr, depth=0):
        w = self.word(addr)
        f = struct.unpack("<f", struct.pack("<I", w))[0]
        if CFSTRING[0] <= w < CFSTRING[1]:
            ptr = self.word(w + 8)
            return f'"{self.cstring(ptr)}"'
        if SELREFS[0] <= w < SELREFS[1]:
            return f"@selector({self.cstring(self.word(w))})"
        if CSTRING[0] <= w < CSTRING[1]:
            return f'cstr "{self.cstring(w)}"'
        if MIN_ADDR <= w < MAX_ADDR and depth < 2:
            sym = self.symbol(w)
            inner = self.describe(w, depth + 1)
            return f"-> {w:#x} {sym or ''} [{inner}]"
        if 1e-5 < abs(f) < 1e7 or f == 0.0 and w == 0:
            return f"{f:g}"
        return f"{w:#x} ({w})"


def main():
    binary = Binary()
    for path in sys.argv[1:]:
        text = Path(path).read_text()
        print(f"== {path}")
        for ref in sorted(set(re.findall(r"DAT_([0-9a-f]{8})", text))):
            addr = int(ref, 16)
            try:
                print(f"DAT_{ref}: {binary.describe(addr)}")
            except Exception as err:  # out-of-range literal
                print(f"DAT_{ref}: ? ({err})")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Make a touchHLE-runnable copy of the original app bundle.

touchHLE v0.3.0 does not implement sched_get_priority_min or
pthread_setschedparam, which Audio::backgroundLoad calls to raise the audio
loader thread's priority. That is only a scheduling hint, so the calls are
replaced with NOPs in the copy. original/ is never modified.

Usage: tools/emu_prepare.py <dst_dir>
Writes <dst_dir>/Meteor Blitz.app
"""

import shutil
import struct
import sys
from pathlib import Path

SRC = Path("original/app")
TEXT_ADDR = 0x2DEC
TEXT_OFFSET = 7660
NOP = 0xE1A00000  # mov r0, r0

# (address, expected word) for each instruction replaced with a NOP.
PATCHES = [
    (0x43714, 0xE3A00004),  # mov r0, #4
    (0x43718, 0xEB011826),  # bl sched_get_priority_min
    (0x4371C, 0xE3A01004),  # mov r1, #4
    (0x43720, 0xE28D2028),  # add r2, sp, #40
    (0x43724, 0xE58D0028),  # str r0, [sp, #0x28]
    (0x43728, 0xE59D0030),  # ldr r0, [sp, #0x30]
    (0x4372C, 0xEB011815),  # bl pthread_setschedparam
]


def main():
    dst = Path(sys.argv[1]) / "Meteor Blitz.app"
    if dst.exists():
        shutil.rmtree(dst)
    shutil.copytree(SRC, dst)
    binary = dst / "Meteor Blitz"
    data = bytearray(binary.read_bytes())
    for addr, expected in PATCHES:
        off = TEXT_OFFSET + addr - TEXT_ADDR
        (word,) = struct.unpack_from("<I", data, off)
        if word != expected:
            sys.exit(f"unexpected word {word:#010x} at {addr:#x}, wanted {expected:#010x}")
        struct.pack_into("<I", data, off, NOP)
    binary.write_bytes(bytes(data))
    print(f"wrote {dst} ({len(PATCHES)} instructions patched)")


if __name__ == "__main__":
    main()

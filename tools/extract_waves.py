#!/usr/bin/env python3
"""Extract the stage scripts (wave and formation tables) from the iOS binary.

The original keeps every arcade stage, the tutorial and survival as static
tables of 32-byte `Wave` records (`_wavesArcade<W>Stage<S>`, `_wavesTutorial`,
`_wavesSurvival`), each pointing at a `Pattern` (formation): a pointer to
24-byte spawn entries terminated by a tick of -1, plus a mode word
(0 = polar angle/distance, 1 = cartesian x/y). Wave counts per stage come
from `WaveManager::startStage`.

Wave record: int type, Pattern* pattern, uint flags, float offset_angle,
float offset_distance, float scale, int ticks_per_step, float threshold.
Spawn entry: int tick, float a, float a_jitter, float b, float b_jitter,
float heading.

Writes src/waves.rs (generated; do not edit by hand).
Usage: tools/extract_waves.py
"""

import struct
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "original" / "app" / "Meteor Blitz"
OUT = ROOT / "src" / "waves.rs"
VM_TO_FILE = 0x1000

# WaveManager::startStage: number of waves per (world, stage).
WAVE_COUNTS = [
    [15, 30, 18, 21, 29],
    [28, 21, 32, 28, 30],
    [26, 37, 29, 33, 37],
    [21, 31, 37, 44, 35],
    [21, 27, 37, 33, 27],
    [25, 23, 20, 28, 34],
]
TUTORIAL_WAVES = 1
SURVIVAL_WAVES = 0x72  # WaveManager::WaveManager for level 6


def symbols():
    out = subprocess.run(["nm", "-n", str(BIN)], capture_output=True, text=True, check=True).stdout
    table = {}
    for line in out.splitlines():
        parts = line.split()
        if len(parts) == 3:
            table[parts[2]] = int(parts[0], 16)
    return table


def main():
    data = BIN.read_bytes()
    syms = symbols()
    by_addr = {addr: name for name, addr in syms.items()}

    def words(addr, fmt):
        return struct.unpack_from(fmt, data, addr - VM_TO_FILE)

    patterns = {}  # address -> (name, mode, spawns)

    def pattern(addr):
        if addr in patterns:
            return patterns[addr][0]
        name = by_addr.get(addr, f"_pattern_{addr:x}").lstrip("_")
        entries_ptr, mode = words(addr, "<Ii")
        spawns = []
        p = entries_ptr
        while True:
            tick = words(p, "<i")[0]
            if tick == -1:
                break
            spawns.append((tick,) + words(p + 4, "<5f"))
            p += 24
            if len(spawns) > 512:
                raise SystemExit(f"runaway pattern at {addr:#x}")
        patterns[addr] = (name, mode, spawns)
        return name

    def waves(symbol, count):
        base = syms[symbol]
        out = []
        for i in range(count):
            kind, pat, flags = words(base + i * 32, "<iII")
            off_angle, off_dist, scale = words(base + i * 32 + 12, "<3f")
            step = words(base + i * 32 + 24, "<i")[0]
            threshold = words(base + i * 32 + 28, "<f")[0]
            out.append((kind, pattern(pat), flags, off_angle, off_dist, scale, step, threshold))
        return out

    stages = [[waves(f"_wavesArcade{w + 1}Stage{s + 1}", n) for s, n in enumerate(row)] for w, row in enumerate(WAVE_COUNTS)]
    tutorial = waves("_wavesTutorial", TUTORIAL_WAVES)
    survival = waves("_wavesSurvival", SURVIVAL_WAVES)
    # _waveSurvivalWait and _waveSurvivalDepot are filled in at run time by a
    # static initializer, so they cannot be read from the file.

    def f(v):
        s = repr(float(v))
        return s if "." in s or "e" in s else s + ".0"

    lines = [
        "//! Stage scripts extracted from the iOS binary by tools/extract_waves.py.",
        "//! Generated; do not edit by hand.",
        "",
        "#![allow(dead_code)]",
        "",
        "/// One spawn in a formation: at `tick` (scaled by the wave's",
        "/// ticks_per_step), at polar (angle, distance) or cartesian (x, y) with",
        "/// symmetric jitter, facing `heading` (used by arrows).",
        "pub struct Spawn {",
        "    pub tick: i32,",
        "    pub a: f32,",
        "    pub a_jitter: f32,",
        "    pub b: f32,",
        "    pub b_jitter: f32,",
        "    pub heading: f32,",
        "}",
        "",
        "pub struct Pattern {",
        "    /// 0: (angle degrees, distance); 1: (x, y).",
        "    pub mode: i32,",
        "    pub spawns: &'static [Spawn],",
        "}",
        "",
        "pub struct Wave {",
        "    /// 0 wait, 1-12 rocks (normal/ice/fire x small..extra large), 13 shy guy,",
        "    /// 14 arrow, 16 snake, 17 mine, 18 seeker, 19 tower, 20 lobber, 21 depot,",
        "    /// 22 boss, 26-29 a rock of random element in size small..extra large.",
        "    pub kind: i32,",
        "    pub pattern: &'static Pattern,",
        "    /// 1: no random rotation; 2: spawned enemies move with the ship;",
        "    /// 4: re-centre on the ship per spawn step; 8: the stage ends when",
        "    /// this wave's kind is cleared; 16: keep the previous wave's origin.",
        "    pub flags: u32,",
        "    pub offset_angle: f32,",
        "    pub offset_distance: f32,",
        "    pub scale: f32,",
        "    pub ticks_per_step: i32,",
        "    /// Units (in small-rock equivalents) to destroy before the next wave.",
        "    pub threshold: f32,",
        "}",
        "",
    ]
    for addr in sorted(patterns):
        name, mode, spawns = patterns[addr]
        const = name.upper()
        lines.append(f"pub static {const}: Pattern = Pattern {{ mode: {mode}, spawns: &[")
        for tick, a, aj, b, bj, h in spawns:
            lines.append(f"    Spawn {{ tick: {tick}, a: {f(a)}, a_jitter: {f(aj)}, b: {f(b)}, b_jitter: {f(bj)}, heading: {f(h)} }},")
        lines.append("] };")
    lines.append("")

    def wave_list(name, items):
        lines.append(f"pub static {name}: &[Wave] = &[")
        for kind, pat, flags, oa, od, scale, step, threshold in items:
            lines.append(
                f"    Wave {{ kind: {kind}, pattern: &{pat.upper()}, flags: {flags}, offset_angle: {f(oa)}, "
                f"offset_distance: {f(od)}, scale: {f(scale)}, ticks_per_step: {step}, threshold: {f(threshold)} }},"
            )
        lines.append("];")

    for w, row in enumerate(stages):
        for s, items in enumerate(row):
            wave_list(f"ARCADE_{w + 1}_{s + 1}", items)
    wave_list("TUTORIAL", tutorial)
    wave_list("SURVIVAL", survival)
    lines.append("")
    lines.append("/// Arcade stage scripts by world and stage (0-based).")
    lines.append("pub static ARCADE: [[&[Wave]; 5]; 6] = [")
    for w in range(6):
        lines.append("    [" + ", ".join(f"ARCADE_{w + 1}_{s + 1}" for s in range(5)) + "],")
    lines.append("];")
    OUT.write_text("\n".join(lines) + "\n")
    total = sum(len(items) for row in stages for items in row)
    print(f"wrote {OUT}: {len(patterns)} patterns, {total} arcade waves, {len(survival)} survival waves")


if __name__ == "__main__":
    main()

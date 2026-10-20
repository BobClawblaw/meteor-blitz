#!/usr/bin/env python3
"""Render the port 1:1 and diff it against an emulator reference frame.

Runs the native build at 480x320 with MB_SHOT (the port saves its 20th
frame and exits), optionally scanning MB_SCROLL values to find the backdrop
phase that best matches a reference taken at an unknown time.

Usage:
  tools/compare_frame.py REF.png [--scan START STOP STEP] [--mask x0,y0,x1,y1 ...]
      --mask rectangles are excluded from the score (e.g. moving UI).
Writes shots/compare.png (reference | port | difference) for the best run.
"""

import argparse
import os
import subprocess
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageChops

ROOT = Path(__file__).resolve().parent.parent
EXE = ROOT / "target" / "debug" / "meteor-blitz"
SHOT = ROOT / "shots" / "compare_port.png"


def render(scroll: float | None, extra=()) -> Image.Image:
    env = dict(os.environ, MB_WINDOW="480x320", MB_SHOT=str(SHOT))
    for item in extra:
        name, value = item.split("=", 1)
        env[name] = value
    if scroll is not None:
        env["MB_SCROLL"] = str(scroll)
    SHOT.unlink(missing_ok=True)
    subprocess.run([str(EXE)], env=env, cwd=ROOT, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=60)
    return Image.open(SHOT).convert("RGB")


def score(ref: np.ndarray, port: np.ndarray, masks) -> float:
    diff = np.abs(ref.astype(int) - port.astype(int)).mean(axis=2)
    keep = np.ones(diff.shape, dtype=bool)
    for x0, y0, x1, y1 in masks:
        keep[y0:y1, x0:x1] = False
    return float(diff[keep].mean())


def phase_peak(ref: np.ndarray, port: np.ndarray, region) -> float:
    """Phase-correlation peak of a region: high when the two images share
    structure at the same scale, whatever the brightness or scroll phase.
    Returned negated so that lower is better, like `score`."""
    x0, y0, x1, y1 = region
    a = ref[y0:y1, x0:x1].mean(axis=2).astype(float)
    b = port[y0:y1, x0:x1].mean(axis=2).astype(float)
    a -= a.mean()
    b -= b.mean()
    r = np.fft.fft2(a) * np.conj(np.fft.fft2(b))
    r /= np.abs(r) + 1e-9
    return -float(np.real(np.fft.ifft2(r)).max())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("ref")
    ap.add_argument("--scan", nargs=3, type=float, metavar=("START", "STOP", "STEP"))
    ap.add_argument("--mask", action="append", default=[], help="x0,y0,x1,y1 to exclude")
    ap.add_argument("--phase", help="x0,y0,x1,y1: score by phase-correlation peak of this region instead")
    ap.add_argument("--env", action="append", default=[], help="extra NAME=VALUE for the port")
    args = ap.parse_args()
    masks = [tuple(int(v) for v in m.split(",")) for m in args.mask]
    ref_im = Image.open(args.ref).convert("RGB")
    ref = np.asarray(ref_im)
    if ref_im.size != (480, 320):
        sys.exit("reference must be 480x320")

    candidates = [None]
    if args.scan:
        start, stop, step = args.scan
        candidates = list(np.arange(start, stop, step))
    best = None
    for scroll in candidates:
        port_im = render(scroll, args.env)
        if args.phase:
            s = phase_peak(ref, np.asarray(port_im), tuple(int(v) for v in args.phase.split(",")))
        else:
            s = score(ref, np.asarray(port_im), masks)
        print(f"scroll={scroll}: mean abs diff {s:.2f}")
        if best is None or s < best[0]:
            best = (s, scroll, port_im)
    s, scroll, port_im = best
    print(f"best: scroll={scroll} diff {s:.2f}")
    out = Image.new("RGB", (480 * 3, 320))
    out.paste(ref_im, (0, 0))
    out.paste(port_im, (480, 0))
    out.paste(ImageChops.difference(ref_im, port_im), (960, 0))
    out.save(ROOT / "shots" / "compare.png")


if __name__ == "__main__":
    main()

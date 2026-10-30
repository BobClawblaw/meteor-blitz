#!/usr/bin/env python3
"""Cleaned copies of the original audio for the game (assets/audio/).

original/audio/*.wav are the app's raw PCM, byte for byte. Played as they are
they click and crackle, for reasons this script fixes:

- 44,200 Hz: the app told OpenAL 44,200 Hz. quad-snd (macroquad's audio)
  converts anything that is not 44,100 Hz with a nearest-sample resampler
  whose float32 index jitters: on the music it repeats or drops about
  356,000 samples in two minutes. These copies are resampled once, properly
  (a 256-tap windowed-sinc filter), to 44,100 Hz, so quad-snd plays them
  untouched and the pitch is the original's.
- Hard clipping: the music and several effects have runs of full-scale
  samples (1,951 in the music). Each run of two or more full-scale samples
  is rebuilt with a cubic fitted to the CONTEXT samples on either side
  (ffmpeg's adeclip was tried and rejected: it also reshapes unclipped
  transients, taking 9-12 dB off the button click), and a lookahead limiter holds the rebuilt peaks under PEAK, so loudness stays
  the original's (turning whole files down would cost up to 4.7 dB).
- DC offset (up to ~1,000 in sound_02): a 15 Hz high-pass removes it.
- Edge steps: effects that end away from zero (sound_17 at -1,353) click
  when they stop; effects get a FADE_OUT fade-out. Every file starts within
  11 of zero, so there is no fade-in, which would soften the attacks.
- The music's loop: it ends at -2,992 and starts at -11, a step at every
  loop. Its last LOOP_FADE seconds are crossfaded (equal power) into its
  first LOOP_FADE seconds, which are dropped, so the end runs into the start.

Writes assets/audio/<name>.wav, 44,100 Hz mono 16-bit, TPDF-dithered and
deterministic. tools/web_audio.py encodes the web MP3s from these.

  tools/clean_audio.py     (needs ffmpeg and numpy)
"""

import subprocess
import wave
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "original/audio"
OUT = ROOT / "assets/audio"
RATE = 44100
PEAK = 10 ** (-1 / 20)  # -1 dBFS
FADE_OUT = 0.005
LOOP_FADE = 0.02
# Samples at or past this are clipped; CONTEXT samples each side fit the rebuild.
CLIP = 32767 / 32768
CONTEXT = 4


def declip(x: np.ndarray) -> np.ndarray:
    """Rebuild each run of 2+ full-scale samples from its neighbours."""
    x = x.copy()
    clipped = np.abs(x) >= CLIP
    i = 0
    while i < len(x):
        if not clipped[i]:
            i += 1
            continue
        j = i
        while j < len(x) and clipped[j]:
            j += 1
        if j - i >= 2 and i >= CONTEXT and j + CONTEXT <= len(x):
            t = np.r_[np.arange(i - CONTEXT, i), np.arange(j, j + CONTEXT)]
            curve = np.polyfit(t - i, x[t], 3)
            rebuilt = np.polyval(curve, np.arange(j - i))
            # A peak keeps the clipped side and never falls below the clip.
            x[i:j] = np.sign(x[i]) * np.maximum(np.abs(rebuilt), CLIP)
        i = j
    return x


def ffmpeg_clean(path: Path) -> np.ndarray:
    """Declip, then remove DC, limit and resample, as float samples."""
    with wave.open(str(path)) as w:
        rate = w.getframerate()
        x = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2") / 32768.0
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-f", "f32le", "-ar", str(rate), "-ac", "1", "-i", "-", "-af",
         f"highpass=f=15,alimiter=limit={PEAK}:attack=2:release=40:level=false:latency=true,aresample={RATE}:filter_size=256:phase_shift=12:cutoff=0.97",
         "-f", "f32le", "-ac", "1", "-"],
        input=declip(x).astype("<f4").tobytes(), check=True, capture_output=True,
    ).stdout
    return np.frombuffer(raw, dtype="<f4").astype(np.float64)


def fade(n: int) -> np.ndarray:
    """Raised-cosine ramp 0 -> 1 over n samples."""
    return 0.5 - 0.5 * np.cos(np.pi * (np.arange(n) + 0.5) / n)


def loop_seam(x: np.ndarray) -> np.ndarray:
    n = int(LOOP_FADE * RATE)
    t = (np.arange(n) + 0.5) / n
    out = x[n:].copy()
    out[-n:] = x[-n:] * np.cos(t * np.pi / 2) + x[:n] * np.sin(t * np.pi / 2)
    return out


def write(path: Path, x: np.ndarray, seed: int) -> None:
    rng = np.random.default_rng(seed)
    dither = (rng.random(len(x)) - rng.random(len(x)))  # TPDF, +-1 LSB
    pcm = np.clip(np.round(x * 32767 + dither), -32768, 32767).astype("<i2")
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(pcm.tobytes())


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for i, wav in enumerate(sorted(SRC.glob("*.wav"))):
        x = ffmpeg_clean(wav)
        if wav.stem == "music":
            x = loop_seam(x)
        else:
            n = int(FADE_OUT * RATE)
            x[-n:] *= fade(n)[::-1]
        # The resampler can overshoot the limiter by a hair.
        x *= min(1.0, PEAK / np.abs(x).max())
        write(OUT / wav.name, x, seed=i)
        print(f"{wav.name}: {len(x) / RATE:.2f} s")


if __name__ == "__main__":
    main()

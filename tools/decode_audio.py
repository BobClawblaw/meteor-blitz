#!/usr/bin/env python3
"""Split the original sound containers into WAV files.

`sounds.raw` and `music.raw` are streams of records read by `Audio::load`:
int id, int byte size, int voices (simultaneous plays), int OpenAL format
(0x1101 = mono 16-bit), int sample rate, then the PCM data.

Writes original/audio/sound_<id>.wav (ids 1-17) and original/audio/music.wav
(id 0), plus original/audio/voices.txt with each sound's voice count.
Usage: tools/decode_audio.py
"""

import struct
import wave
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "original" / "app"
OUT = ROOT / "original" / "audio"
FORMATS = {0x1100: (1, 1), 0x1101: (1, 2), 0x1102: (2, 1), 0x1103: (2, 2)}  # channels, bytes


def records(path):
    data = path.read_bytes()
    off = 0
    while off + 20 <= len(data):
        sid, size, voices, fmt, rate = struct.unpack_from("<5i", data, off)
        yield sid, voices, fmt, rate, data[off + 20 : off + 20 + size]
        off += 20 + size


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    voices_lines = ["# sound id -> simultaneous voices (Audio::load)"]
    for container in ("sounds.raw", "music.raw"):
        for sid, voices, fmt, rate, pcm in records(SRC / container):
            channels, width = FORMATS[fmt]
            name = "music.wav" if sid == 0 else f"sound_{sid:02}.wav"
            with wave.open(str(OUT / name), "wb") as w:
                w.setnchannels(channels)
                w.setsampwidth(width)
                w.setframerate(rate)
                w.writeframes(pcm)
            voices_lines.append(f"{sid} {voices}")
            print(f"{name}: {len(pcm) / (rate * channels * width):.2f} s, {voices} voices")
    (OUT / "voices.txt").write_text("\n".join(voices_lines) + "\n")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Compressed audio for the web build (desktop builds keep the WAVs).

Encodes original/audio/*.wav to MP3 (LAME VBR, which every browser's
decodeAudioData reads, Safari included; the LAME header carries the encoder
delay and padding so the looping music decodes without a gap) into
assets/web-audio/. The web build loads these instead of the 11 MB of WAVs
(src/main.rs `sound_path`), and tools/web_pack.py packs them.

  tools/web_audio.py     (needs ffmpeg with libmp3lame)
"""

import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "original/audio"
OUT = ROOT / "assets/web-audio"
# LAME VBR quality: 2 (~190 kbps) for the music, 4 (~165 kbps) for effects.
MUSIC_QUALITY = "2"
EFFECT_QUALITY = "4"


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    total_in = total_out = 0
    for wav in sorted(SRC.glob("*.wav")):
        mp3 = OUT / (wav.stem + ".mp3")
        quality = MUSIC_QUALITY if wav.stem == "music" else EFFECT_QUALITY
        subprocess.run(
            ["ffmpeg", "-v", "error", "-y", "-i", str(wav), "-codec:a", "libmp3lame",
             "-q:a", quality, "-map_metadata", "-1", "-fflags", "+bitexact", str(mp3)],
            check=True,
        )
        total_in += wav.stat().st_size
        total_out += mp3.stat().st_size
    print(f"{OUT.relative_to(ROOT)}: {total_in:,} bytes of WAV -> {total_out:,} bytes of MP3")


if __name__ == "__main__":
    main()

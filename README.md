# Meteor Blitz — modern port

A modern port of Alley Labs' *Meteor Blitz* (2009), built from the original iOS 1.4 assets: planet-surface terrain, UI art, and the original soundtrack and SFX.

Targets: **web (canvas/WASM)**, **macOS**, **Windows**. Controls: WASD + mouse.

## Provenance

- Source: `original/Meteor_Blitz_1.4_ios_2.1.ipa` (iOS 1.4, iOS 2.1), archived at the Internet Archive.
- `original/app/` — the extracted `.app` bundle, untouched.
- `original/textures/` — the 11 PVRTC planet/fog/cloud textures decoded to PNG.
- `original/audio/` — `sounds.raw` and `music.raw` decoded to WAV (mono 44.1 kHz PCM).

`original/` is read-only source material. Never edit it; regenerate derived assets instead.

## What is in the original

- Planet surface tiles: `crust`, `craters`, `sprawl`, `blue`, `green`, `fire`, `clouds`, `fogRed`, `fogGreen`, `fogBlackRed`, `space4deep`.
- UI: 187 PNGs (buttons, labels, achievements, stage select).
- Audio: 10.7 s of SFX and a 120 s music track.
- Gameplay sprites are PVRTC-compressed inside the binary/`systems.dat`; the decoded terrain and UI art are the usable set.

## Port plan

See `docs/port-plan.md`. Short version: Rust + macroquad, one codebase for web/macOS/Windows, large wrapping world with camera, original terrain and audio, faithful weapon set (plasma / flame / ice / gravity grab-sling / bombs / boost), meteor kinds with the weakness matrix, persistent chunks, enemies and bosses, 30 levels.

## Build

```bash
cargo run            # native window
cargo build --release
```

Web build instructions land with the first Rust milestone.

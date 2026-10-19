# Meteor Blitz port — plan

**Goal:** A modern port of Meteor Blitz built on the original assets: large wrapping planet-surface world, faithful weapons and enemies, 30 levels, web + macOS + Windows.

**Stack:** Rust + macroquad (one codebase, native and WASM web build). Original terrain textures and audio; procedural sprites where the original art is PVRTC-only.

## Phases

1. **Scaffold** — Rust crate, window, fixed-step loop, camera, asset loading, terrain tiling with wrap.
2. **World** — 5760x3240 wrapping world, camera follow, planet surface layers (crust/craters/fog), horizon.
3. **Ship** — movement, dash/boost, plasma/flame/ice/gravity, bombs, heat, upgrades.
4. **Meteors** — kinds (rock/ice/fire), weakness matrix, shatter into persistent chunks, ore.
5. **Enemies** — drones, turrets, slingers, bosses.
6. **Levels** — 6 worlds x 5 stages, wave director, survival mode.
7. **HUD** — modern minimal HUD, radar, stage select using original UI art.
8. **Audio** — slice the original SFX sequence, music track, reactive layers.
9. **Desktop/web** — macOS app, Windows build on `xamd4090`, WASM web build.
10. **QA** — perf, playtest, release.

## Asset notes

- Terrain: `original/textures/*.png` (decoded from PVRTC). Tint per world.
- UI: `original/app/*.png` for stage select and menus.
- Audio: `original/audio/sounds.wav` (SFX sequence, needs slicing), `music.wav` (120 s track).
- Gameplay sprites: not in PNG form; draw procedurally in the port's style, or decode the embedded PVRTC later.

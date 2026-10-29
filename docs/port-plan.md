# Meteor Blitz port — plan

**Goal (revised 2026-10-20):** First a pixel-perfect port of the original iOS game, then improvements on top of it. Web + macOS + Windows.

**Stack:** Rust + macroquad (one codebase, native and WASM web build), original assets only.

## Faithful-port approach

- **Reference is the original binary.** `original/app/Meteor Blitz` is a decrypted ARMv6 Mach-O (cryptid 0) with its C++ symbols intact: `MainMenuScreen`, `BasicScreen`, `Background`, `World`, `Ship`, `GravityGun`, `WaveManager`, `EnemyManager`, `Snake`, `Seeker`, `ShyGuy`, `Lobber`, `Mine`, `Tower`, `Arrow`, `BossInside/Outside`, `Hud`, `Upgrade`/`Depot`, etc. Layouts, tuning and behaviour are translated from it, not guessed.
- **Canvas:** 2D is a 480x320, y-down, top-left canvas (`Texture::draw(x, y, scale)` draws top-left anchored at image size x scale). The game world and menu backdrop are 3D: `Director` sets a perspective camera (`gluLookAt`, `glFrustumf`, rotated -90 for landscape) and `Background` is a curved mesh (`World::getOffsetZ`) multitexturing a base layer with a fog layer (unit 1 `GL_DECAL`).
- **Tools:** `llvm-objdump --triple=armv6-apple-ios --mattr=+vfp2` works for spot reads. A decompiler (Ghidra) is wanted for the full game logic, and an emulator (touchHLE) for ground-truth 480x320 frames to diff against.
- **Status:** main menu 2D layout exact (title at ((480-w)/2, 15); PLAY 160,176; HIGH SCORES 16,176; SETTINGS 336,176; HELP 336,246). Menu backdrop is a flat approximation of the 3D `Background`.
- Pause, controls, upgrade, clear and game-over popups ported from `PausePopup`; layouts and gradients match the decompiled draw code and are verified with `MB_PAUSE` captures. `PausePopup` settings persist to `settings.txt` (`save`/`load`), checked headlessly with `MB_DUMP_SETTINGS`.
- High-score tables (`LocalScores`) live in `scores.txt`. `HighScoresSelectScreen` picks arcade or survival; `HighScoresScreen` draws the rows under the original labels with Local/Global tabs, and the Global tab reports offline (Game Center is gone).
- The HELP button opens the tutorial page with the original control diagram (`keyboard.png`).
- The earlier reimagined gameplay (wrapping 5760x3240 world, drones/turrets/slingers, heat, dash) is not the original's and will be replaced screen by screen.

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

## Performance

- The loop is paced at 60 Hz (the original's simulation rate); `MB_FPS=0` disables the cap for benchmarks, `MB_PROF`/`MB_PROF_FRAMES` report update/draw CPU (see `AGENTS.md`).
- Platform input and per-frame test hooks are read once, not per frame.

## Asset notes

- Terrain: `original/textures/*.png` (decoded from PVRTC). Tint per world.
- UI: `original/app/*.png` for stage select and menus.
- Audio: `original/audio/sounds.wav` (SFX sequence, needs slicing), `music.wav` (120 s track).
- Gameplay sprites: 520 frames decoded from `game.raw` into `original/sprites/`.
- Menu art: CgBI PNGs in `original/app/`, decoded by `tools/decode_ui.py` into `assets/ui/`.

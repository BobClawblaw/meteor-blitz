# Meteor Blitz port — plan

**Goal (revised 2026-10-29):** A modern port. The simulation, tuning and content stay as decompiled from the original; presentation and input are modern: any window size and aspect (widescreen and ultrawide fill the screen), fullscreen, a window/render resolution from 854x480 up to 4K, HD art, post-processing, rebindable keyboard and controller input, opt-in tutorial. Emulator pixel diffs are no longer a goal; `MB_WIDE=0 MB_HD=0 MB_RENDER_SCALE=1` keeps the original view for reference checks. Web + macOS + Windows.

(Earlier goal, 2026-10-20: a pixel-perfect port first, then improvements.)

**Stack:** Rust + macroquad (one codebase, native and WASM web build), original assets only.

## Faithful-port approach

- **Reference is the original binary.** `original/app/Meteor Blitz` is a decrypted ARMv6 Mach-O (cryptid 0) with its C++ symbols intact: `MainMenuScreen`, `BasicScreen`, `Background`, `World`, `Ship`, `GravityGun`, `WaveManager`, `EnemyManager`, `Snake`, `Seeker`, `ShyGuy`, `Lobber`, `Mine`, `Tower`, `Arrow`, `BossInside/Outside`, `Hud`, `Upgrade`/`Depot`, etc. Layouts, tuning and behaviour are translated from it, not guessed.
- **Canvas:** 2D is a 480x320, y-down, top-left canvas (`Texture::draw(x, y, scale)` draws top-left anchored at image size x scale). The game world and menu backdrop are 3D: `Director` sets a perspective camera (`gluLookAt`, `glFrustumf`, rotated -90 for landscape) and `Background` is a curved mesh (`World::getOffsetZ`) multitexturing a base layer with a fog layer (unit 1 `GL_DECAL`).
- **Tools:** `llvm-objdump --triple=armv6-apple-ios --mattr=+vfp2` works for spot reads. A decompiler (Ghidra) is wanted for the full game logic, and an emulator (touchHLE) for ground-truth 480x320 frames to diff against.
- **Status:** main menu 2D layout exact (title at ((480-w)/2, 15); PLAY 160,176; HIGH SCORES 16,176; SETTINGS 336,176; HELP 336,246). Menu backdrop is a flat approximation of the 3D `Background`.
- Pause, controls, upgrade, clear and game-over popups ported from `PausePopup`; layouts and gradients match the decompiled draw code and are verified with `MB_PAUSE` captures. `PausePopup` settings persist to `settings.txt` (`save`/`load`), checked headlessly with `MB_DUMP_SETTINGS`.
- High-score tables (`LocalScores`) live in `scores.txt`. `HighScoresSelectScreen` picks arcade or survival; `HighScoresScreen` draws the rows under the original labels with Local/Global tabs, and the Global tab reports offline (Game Center is gone).
- `labelEnterName` opens before the results popup when a run makes the table; the desktop port reads the keyboard directly instead of the original on-screen keyboard.
- The HELP button opens the tutorial page with the original control diagram (`keyboard.png`) and the caption block (`captions.png`).
- `Tutorial::canShow*` hints show the caption block once per session on the first score, multiplier 2, bomb use and lost life.
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

## Modern-port additions (2026-11-02)

In order, one commit each. Out of scope (decided 2026-11-03): macOS notarization and Windows code signing (no developer account; the apps stay ad hoc signed), and an online leaderboard (the Global scores tab stays "offline").

1. Frame rate: 165/240 Hz caps and an uncapped (VSync) choice.
2. HUD scale option.
3. Accessibility: reduce flashes (full-screen flashes, shockwaves), colourblind-safe tints for fire/ice and indicators.
4. Quick restart from the pause menu; replay any cleared stage from stage select.
5. Controller button icons (Xbox and PlayStation) in prompts and on the Controls page. Done: Xbox, PlayStation and Nintendo styles, Auto by the connected pad.
6. Rumble on bombs, hits, deaths and boss kills. Done: Options > Vibration; macOS Core Haptics, gilrs force feedback, the web's dual-rumble.
7. Local stats page and achievements. Done: lifetime stats (`stats.txt`) and 23 achievements on a Stats & achievements page from the main menu, with unlock toasts in play.
8. Touch controls for the web build (twin sticks, bomb, boost). Done: floating sticks on the original wheel art, bomb and boost buttons placed by the original Boost/Bomb settings.
9. Linux package (and Steam Deck check). Done: `tools/package-linux.sh` (Debian 11 container, glibc 2.30), Steam Deck defaults; checked under Xvfb at 1280x800, not yet on a Deck.

## Graphics quality overhaul (2026-11-27)

Full visual upgrade on top of the faithful simulation core (keeping `MB_WIDE=0 MB_HD=0 MB_RENDER_SCALE=1 MB_BLOOM=0 MB_FX_OFF=1` intact as the classic reference):

1. **8x Art Overhaul & Mipmaps:** Replace xBR 4x+2x on `assets/8x/` sprites, UI, and font with edge-inpainted Real-ESRGAN + smooth alpha upscaling (eliminating onion-ring halos and painterly smearing); add trilinear mipmaps and parallel predecoding for HD sprites/UI; smooth filtered HD font rendering.
2. **Pyramid Bloom & ACES Post-FX:** 5-level progressive downsample/upsample bloom pyramid, shockwave-distorted bloom composition, calibrated ACES filmic tone mapping, and 1-LSB gradient dithering.
3. **Planet Surface & Atmosphere:** Cloud drop shadows on terrain, dynamic light illumination on clouds, world-specific materials (ocean specular glint, pulsing molten lava glow, crater relief, atmospheric Fresnel limb haze), and a crisp procedural multi-layer twinkling parallax starfield for space worlds and menus.
4. **3D Depth & Per-Pixel Sprite Lighting:** Parallax-offset ground drop shadows on planet worlds, plus per-pixel bump+dome normal sprite lighting with 12 dynamic point lights, material-aware specular/Fresnel rim highlights, and emissive self-illumination on fire rocks, cores, and engines.
5. **Procedural Energy Shaders & Lit Particles:** Animated hexagonal Fresnel shield shader, pulsing tractor-beam gravity gun shader, tapered afterburner engine ribbon, muzzle flashes, cooling molten-ember scorch marks, and explosion-illuminated smoke in `src/bursts.rs`.

## Performance

- The loop is paced at 60 Hz (the original's simulation rate); `MB_FPS=0` disables the cap for benchmarks, `MB_PROF`/`MB_PROF_FRAMES` report update/draw CPU (see `AGENTS.md`).
- Platform input and per-frame test hooks are read once, not per frame.

## Asset notes

- Terrain: `original/textures/*.png` (decoded from PVRTC). Tint per world.
- UI: `original/app/*.png` for stage select and menus.
- Audio: `original/audio/sounds.wav` (SFX sequence, needs slicing), `music.wav` (120 s track).
- Gameplay sprites: 520 frames decoded from `game.raw` into `original/sprites/`.
- Menu art: CgBI PNGs in `original/app/`, decoded by `tools/decode_ui.py` into `assets/ui/`.

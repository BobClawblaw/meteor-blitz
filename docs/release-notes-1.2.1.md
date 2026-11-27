# Meteor Blitz 1.2.1

Follow-up polish release fixing mangled 8× UI/sprite art from v1.2.0 and upgrading the fluid simulation into a dedicated high-resolution GPU **Gas Physics** particle mode.

## Fixed
- **8× UI, Font & Sprite Upscaling (`tools/upscale8x.py`):**
  - Replaced `realesrgan-x4plus-anime` + edge-replicated neural alpha on UI with **reflect-padded `realesrgan-x4plus` + smooth Bicubic alpha**. Fixes cartoonized world/stage cubes (`buttonArcade1..6`, `buttonStage*`, locked cubes), protruding border slivers on `buttonArcade2.png`, distorted `buttonBack.png` corners, cartoon outlines on slider `peg.png`, and glitched upgrade icons (`buttonUpgrade*.png`).
  - Added neutral luminance desaturation for monochrome chrome labels (`labelSelectWorld`, `labelSelectStage`, `labelSelectMode`, `labelSettings`, etc.), eliminating vertical RGB glitch speckles.
  - Switched pure-white packed text atlases (`ngage.png`, `captions.png`) to row-masked smoothstepped bilinear upscaling, reducing font glyph cross-row bleeding from 30/38 to **0/38**.
  - Added an automated downsample-to-1× MAE fidelity check inside `tools/upscale8x.py`.
- **Clean `Physics` Particle Style:**
  - Restored full, crisp Newtonian fire/smoke/sparks/debris in `Options > Particle style: Physics` and removed the old 128×128 CPU gas overlay from `Physics` mode.

## Added
- **New `Gas Physics` Particle Style (`Options > Particle style: Classic / Physics / Gas Physics`):**
  - Full-replacement **GPU toroidal Navier-Stokes fluid-gas simulation** (`src/gpu_fluid.rs`, `src/gas.rs`) running in `RGBA16F` render targets with hardware `GL_REPEAT` wrapping across the 1024×1024 arena:
    - `256×256` velocity, curl vorticity confinement (`58.0`), divergence, and 16-iteration warm-started Jacobi pressure solver.
    - `512×512` HDR plasma & vapour dye with **2nd-order MacCormack error-corrected advection** (min/max clamped for sharp swirling Kelvin-Helmholtz filaments instead of blurry splotches).
    - Asymmetric multi-jet + vortex-dipole blasts, ship thruster exhaust & Karman slipstream wakes, weapon plasma trails, bomb/boss expanding vortex shock rings, and gravity-gun spiral accretion vortices.
    - Central-difference 3D fluid relief normals, 12-light volumetric illumination, filmic HDR filament shading, and direct 5-level pyramid bloom integration.
  - Benchmarked at **152.7 fps** (`0.05 ms` CPU update, `2.17 ms` draw) at native 4K (`3840×2160`) on Apple Silicon.

## Verification
- `cargo test`: **39 / 39 passed** (0 warnings on native and `wasm32-unknown-unknown`).
- Visually verified `SELECT WORLD`, `SELECT STAGE`, `SELECT MODE`, `LOCAL ARCADE`, `PAUSED`, `UPGRADE`, `Physics` bursts, and `Gas Physics` 4K vortices.

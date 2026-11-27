# Meteor Blitz 1.2.2

Removes the experimental fluid gas simulation and glowing ground ember splotches.

## Removed / Fixed
- Completely removed `Gas Physics` fluid simulation (`src/gas.rs`, `src/gpu_fluid.rs`, `GAS_FIELD_FRAGMENT`) and its inverted ship exhaust jet.
- Removed the glowing circular ember overlay on ground scorch marks (`draw_scorches`), restoring clean burn marks.
- Restored `Options > Particle style` cleanly to `Classic` / `Physics` (existing `settings.txt` values referencing `gas` fall back smoothly to `Physics`).
- Keeps all v1.2.1 art fixes (clean reflect-padded Real-ESRGAN 8× UI/sprites, zero-bleed font atlas) and all other v1.2 graphics upgrades (5-level pyramid bloom, ACES tone mapping, planet shaders, PBR sprite lighting, hex shield, tractor beam, plasma ribbon).

## Verification
- `cargo test`: **38 / 38 passed** (0 warnings on native and `wasm32-unknown-unknown`).

# Meteor Blitz 1.2.4

Restores the 3rd particle mode — **`Gas Physics`** (`Options > Particle style: Classic / Physics / Gas Physics`) — with the forward-shooting exhaust bug and lingering muddy splotches fixed.

## Fixed & Improved
- **Restored `Gas Physics` as the 3rd particle mode** (`Classic` / `Physics` / `Gas Physics`; `Physics` stays pure Newtonian particles with zero fluid overlay).
- **Fixed ship exhaust shooting out the front (`src/gas.rs`, `src/play.rs`):**
  - Removed the overlapping `+1.15 × ship_vel` forward wake splat that grabbed engine exhaust and threw it ahead of the ship's nose.
  - Exhaust now emits strictly behind the ship's tail nozzle (`-nose_dir`, interpolated smoothly along the ship's path) and shoots cleanly backward away from the hull.
- **Removed lingering muddy splotches (`src/gpu_fluid.rs`, `src/gfx.rs`):**
  - Removed the separate grey-blue smoke alpha channel that outlived the fire as fading grey splotches; opacity now tracks luminous swirling gas density directly so tendrils dissolve cleanly into space.
  - Replaced per-frame dotted cannon/laser dye splats with pure velocity-only supersonic wakes (slicing swirls through existing gas clouds without dropping dots in empty space), while flamethrower shots emit continuous interpolated flame streams.
  - Kept the v1.2.2 removal of circular glowing ember splotches on ground scorches and all v1.2.3 macOS multi-monitor/HiDPI window fixes.

## Verification
- `cargo test`: **41 / 41 passed** (including `ship_exhaust_shoots_strictly_backward_from_tail` and `window_fit_preserves_aspect_when_clamped_to_display`; 0 warnings on native & `wasm32-unknown-unknown`).
- Visually verified live 60 fps flight in `Gas Physics` mode flying right, up, and down: emerald-cyan gas stream exits cleanly out the tail with zero gas in front of the ship.

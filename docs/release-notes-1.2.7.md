# Meteor Blitz 1.2.7

Full-codebase audit fixes:
- **Fixed quick-tap Gravity Gun cancellation at high FPS:** Tapping `E` / `LT` without holding Fire and releasing before the next 60 Hz tick no longer cancels the gravity gun on the same tick it activated (`src/play.rs`).
- **Fixed Survival Auto-Upgrade toggle (was both hidden and dead):** `Pause` previously checked `self.world == 6` (always `0` on `Pause::menu()`) instead of `play.world == 6`, hiding the Survival Auto-Upgrade button, and `settings.auto_upgrade` was never read by `Play`. The toggle now appears in Pause → Upgrade during Survival and automatically spends rings on the cheapest affordable upgrades (`src/pause.rs`, `src/play.rs`, `src/main.rs`).
- **Restored shockwave rings, ricochet sparks, split rays, and crystal shards in `Gas Physics`:** `Gas Physics` previously skipped all `systems.dat` effects; it now skips only legacy `Point`/`Sprite` dot clouds while keeping crisp `Ring`, `Line`, and `Diamond` effects (`src/particles.rs`, `src/play.rs`).
- **Fixed `Gas Physics` on Web/WASM:** Since the browser build stubs `GpuFluid`, Web/WASM now hides `Gas Physics` from Options and maps any saved `gas` setting to `Physics` so browser players never get blank explosions (`src/pause.rs`, `src/main.rs`).
- **Clear stale GPU fluid on mode switch:** Switching away from `Gas Physics` clears the fluid buffer immediately (`src/main.rs`).

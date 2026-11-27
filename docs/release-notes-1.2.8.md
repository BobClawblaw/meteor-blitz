# Meteor Blitz 1.2.8

- **Toned down flamethrower gas in `Gas Physics`:** Reduced flamethrower in-flight fluid emission (~85% lighter, sampled per-tick heat wisps instead of 2 full-intensity splats per active flame projectile) and added a throttled `flame_impact` so roasting rocks produces a subtle curling heat halo around the flame jet instead of overwhelming the screen (`src/gas.rs`, `src/play.rs`).

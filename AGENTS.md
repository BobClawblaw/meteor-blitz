# AGENTS.md — Meteor Blitz port

## Ground rules

- **No PII in commits or files.** Author identity is `BobClawblaw <BobClawblaw@users.noreply.github.com>`.
- **Serial execution.** One task at a time. No parallel subagent fan-out.
- **Commit after every task.** Conventional commits.
- **Worklog.** Append to `worklog/YYYY-MM-DD.md` before finishing a session: what changed, what was verified with real output, what is next, blockers. Append only.
- **Plan first.** Work from `docs/port-plan.md`; update it when scope changes.

## Asset provenance

- `original/` is read-only. The IPA, the extracted `.app`, decoded textures, and decoded audio are source material.
- Derived assets go in `assets/` and must be regenerable by a script in `tools/`.
- Do not hand-edit decoded PNGs or WAVs; fix the decode script and rerun.

## Repos

- Primary: `nas` → `ssh://agentnas/volume1/storage/git/meteor-blitz.git`
- Secondary mirror: `github` → `BobClawblaw/meteor-blitz` (private)
- Push to both. Never force-push without listing what dies first.

## Windows builds

Run on `xamd4090` (SSH alias, key auth installed). Node 24.14 and git 2.53 present; Rust is not — install rustup before the first Windows build.

## Resume status (2026-10-20)

Done (faithful to the binary unless noted):
- Menus, game screen, HUD, sounds, particle effects (`systems.dat`), rings, multiplier, power-ups.
- Every enemy kind in the wave tables: rocks, depots, mines, shy guys, snakes, arrows, towers, seekers, lobbers, boss.
- Bombs, fire and laser weapons (levels by world), gravity gun.
- Deliberate change from the original: the planet backdrop scrolls at the play plane's screen speed, so rocks stay glued to the ground (`gfx::glued_scroll`). The original's rate let them slide.

Controls:
- Keyboard and mouse: WASD moves, left mouse fires toward the cursor, Space boosts, right mouse or B bombs, Q or the wheel switches weapon.
- Gravity gun: double-click, or E, while firing.
- Game controller: mapped in `src/pad.rs`.

Blocked: the Xbox Series controller (USB, 045e:0b12) on macOS gives no input.
- GameController sees it, but every value reads zero, even as an .app (`tools/make-app.sh`).
- gilrs sees no sticks, and raw IOHID sees no reports.
- First check: System Settings > Game Controllers. If that page reacts, rerun the probes while buttons are pressed.

Next, per `docs/port-plan.md`: pause and results popups, upgrades, survival mode, tutorial, settings, high scores.

Test hooks (env):
- `MB_START=w.s` starts a stage; `MB_WAVE=n` starts at wave n.
- `MB_SHOT` and `MB_SHOT_FRAME` capture a frame and exit.
- `MB_AUTOFIRE`, `MB_GUN`, `MB_BOMB_AT`, `MB_WEAPON` and `MB_DRIFT` drive play unattended.
- `MB_PAD_LOG` prints controller input.
- `cargo test` covers the boss patterns, the gravity gun and the ground scroll.

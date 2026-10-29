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

Run on `xamd4090` (local checkout, or SSH alias with key auth). Node 24.14, git 2.53, MSVC Build Tools and rustup (stable, MSVC) present.
- Cargo lives in the rustup home and may not be on PATH in a fresh shell; prepend `.cargo/bin` under the user profile.
- Long paths break the MSVC linker; keep `CARGO_TARGET_DIR` short and build outside the user profile.
- This checkout has only the `origin` remote (GitHub); the NAS remote is not configured here.
- Controller: Xbox Wireless Controller over Bluetooth LE (045e:0b13). It sleeps when idle and XInput then reports it as not connected; press the Xbox button to wake it.

## Resume status (2026-10-20, Windows controller session)

Done (faithful to the binary unless noted):
- Menus, game screen, HUD, sounds, particle effects (`systems.dat`), rings, multiplier, power-ups.
- Every enemy kind in the wave tables: rocks, depots, mines, shy guys, snakes, arrows, towers, seekers, lobbers, boss.
- Bombs, fire and laser weapons (levels by world), gravity gun.
- Pause, controls, upgrade, clear and game-over popups from `PausePopup`: layouts and gradients match the decompiled draw code; keyboard/mouse navigation; upgrade shop; survival auto-upgrade toggle.
- Deliberate change from the original: the planet backdrop scrolls at the play plane's screen speed, so rocks stay glued to the ground (`gfx::glued_scroll`). The original's rate let them slide.
- Performance: the loop is paced at 60 Hz (the original's simulation rate); macOS controller polling no longer posts a LaunchServices KVO notification every frame.

Approximations / not yet faithful:
- Pause buttons use the generic 26x26 nine-slice plus runtime text instead of the original pre-rendered/dynamic button art; sliders are simplified rectangles.
- `Settings` (music, effects, flip, shake, boost, bomb, auto pause/gravity/upgrade) now persist to `settings.txt`, matching `PausePopup::save/load`; a first launch writes the defaults, and changes are written through as they are made.
- High-score tables keep local arcade and survival rows in `scores.txt`. A run that makes the table opens `labelEnterName` first (typed letters/digits, up to eight; Escape keeps `PLAYER`), and the Global tab reports that the leaderboard is offline, since the original's Game Center server is gone.
- The HELP page shows the original control diagram (`keyboard.png`). The in-game tutorial captions (`captions.png`) are not shown yet.
- Menu fidelity, measured with `tools/compare_frame.py` against `original/screenshots/emu/main_menu.png`: the four main-menu buttons are pixel-exact (mean abs diff 0.04-0.10). The backdrop still differs (whole frame 7.8, backdrop band 9.6) because the emulator frame was taken at an unknown scroll phase and its 3D background is not reproduced exactly.

Controls:
- Keyboard and mouse: WASD moves, left mouse fires toward the cursor, Space boosts, right mouse or B bombs, Q or the wheel switches weapon.
- Gravity gun: double-click, or E, while firing.
- Pause: Esc opens; arrows/WASD move the selection, Enter/A activates, Esc/B goes back; mouse clicks activate items.
- Game controller: mapped in `src/pad.rs`. Windows reads it through gilrs's XInput backend (verified 2026-10-20: A, B, X, Y, LB, RB, LT, RT, D-pad, both sticks full range; View and Start not yet pressed in a logged run). The default WGI backend dropped most input from the Bluetooth LE pad, and its Drop failed fast on window close; `Gilrs` is kept in `ManuallyDrop`.

Blocked (macOS only; Windows works): the Xbox Series controller (USB, 045e:0b12) on macOS gives no input.
- GameController sees it, but every value reads zero, even as an .app (`tools/make-app.sh`).
- gilrs sees no sticks, and raw IOHID sees no reports.
- First check: System Settings > Game Controllers. If that page reacts, rerun the probes while buttons are pressed.

Next:
- The in-game tutorial captions (`captions.png`).
- Pixel-diff the pause/results popups against emulator frames; replace the generic button art with the original pre-rendered/dynamic buttons.
- Resolve controller input on macOS.
- Windows: press View and Start with `MB_PAD_LOG=1` and confirm they log; then play a full stage on the pad (move, aim, RT fire, LT gravity, A/LB boost, B/RB bomb, X/Y weapon, pause navigation).

Test hooks (env):
- `MB_START=w.s` starts a stage; `MB_WAVE=n` starts at wave n.
- `MB_SHOT` and `MB_SHOT_FRAME` capture a frame and exit.
- `MB_PAUSE=<menu|controls|upgrade|clear|gameover>` opens a pause page for screenshot runs.
- `MB_AUTOFIRE`, `MB_GUN`, `MB_BOMB_AT`, `MB_WEAPON` and `MB_DRIFT` drive play unattended.
- `MB_FPS=<n>` overrides the 60 Hz frame cap (`0` = uncapped). `MB_PROF=1` prints update/draw CPU per 120 frames; `MB_PROF_FRAMES=n` exits after n frames for benchmarks. Screenshot runs are uncapped unless `MB_FPS` is given, so `MB_SHOT_FRAME` still selects the same state.
- `MB_PAD_LOG` prints controller input. To tell a game bug from a driver one on Windows, run an XInput reader (`XInputGetState` on xinput1_4.dll) alongside and compare.
- `MB_MENU=<main|mode|help|scores|scores-arcade|scores-survival|scores-global>` opens a menu screen directly, for captures.
- `MB_DUMP_SETTINGS=1` prints the settings loaded from `settings.txt` and exits, for checking persistence without a GUI.
- `MB_NAME=<name>` confirms the `labelEnterName` field for scripted runs; `MB_MENU=name` opens it directly.
- `cargo test` covers the boss patterns, the gravity gun and the ground scroll.


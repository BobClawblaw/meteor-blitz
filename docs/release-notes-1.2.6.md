# Meteor Blitz 1.2.6

- **Fixed dropped weapon-switch and button presses at high/unlimited FPS:** `Play::update` previously polled one-frame edge inputs (`SwitchWeapon`, `Bomb`, `Boost`, `Gravity`, double-tap `Fire`, tutorial quit) only inside the fixed 60 Hz `while accumulator >= STEP` loop. At 120–240 Hz or Unlimited FPS (`fps=0`, ~500–800 fps), 50%–90%+ of render frames did not execute a simulation step, silently dropping almost every weapon switch (`Q`, mouse wheel, `X`/`Y`) and action button press — and slow frames running two steps could double-trigger a single press.
- Edge-triggered gameplay inputs are now latched once per render frame (`PendingInput`) and drained cleanly on the next 60 Hz simulation tick, so 100% of presses register at any frame rate with zero double-firing.

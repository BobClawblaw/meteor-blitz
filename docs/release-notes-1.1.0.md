# Meteor Blitz 1.1.0

New in this release:

- **Linux and Steam Deck.** A Linux build (`MeteorBlitz-1.1.0-linux-x86_64.tar.gz`) for x86_64 distributions from Ubuntu 20.04 and Debian 11 on, and SteamOS. On a Steam Deck the window opens at the screen's 1280x800 and the first launch starts fullscreen.
- **Touch controls in the browser.** On a phone or tablet the web version shows two sticks, built from the original's control wheel: the left thumb moves, the right thumb aims and fires, and a quick double tap on the right reaches out with the gravity gun. Bomb is a button; boost is a quick tap on the left side or a button, set under Pause > Controls as in the original. Menus and the pause icon work by tapping.
- **Stats and achievements.** "Stats & achievements" on the main menu counts rocks, enemies, bosses, rings, bombs, deaths, throws, time played and best scores, and tracks 23 achievements. An unlocked one shows briefly at the bottom of the screen. Progress from earlier versions counts towards the stage achievements.
- **Physics particles.** Options > Particle style, Physics: explosions made of fire, sparks, spinning debris and slow smoke that keep the momentum of the shot that caused them, coloured by what was hit. Classic, the original's effects, stays the default.
- **Playfield zoom.** Options > Playfield zoom shows up to 1.5 times as much of the play field, limited per window shape so nothing appears twice.
- **Frame rate.** Caps of 165 and 240 fps and Unlimited, in addition to 60, 120 and 144. The game itself still runs at 60 steps per second.
- **HUD size.** Options > HUD size enlarges the score, lives and multiplier up to 1.5 times.
- **Accessibility.** Options > Reduce flashes dims full-screen flashes and turns off the shockwave distortion. Options > Colour assist marks ice and fire rocks with symbols, for players who find their colours hard to tell apart.
- **Restart.** The pause menu has a Restart button that starts the stage again with the score, lives and upgrades you entered it with.
- **Controller button icons.** The Controls page and the tutorial show controller buttons as Xbox, PlayStation or Nintendo icons, following the controller in use (or set under Options > Button icons).
- **Vibration.** Bombs, hits on the shield, losing the ship and the boss's death rumble the controller (Options > Vibration).

The web version (https://bobclawblaw.github.io/meteor-blitz/) has all of these except the Linux build and the Steam Deck settings. It keeps stats only for the session.

## Downloads

- **macOS** (`MeteorBlitz-1.1.0-macos.dmg`): universal app for Apple Silicon and Intel, macOS 11 or later. Open the DMG and drag Meteor Blitz to Applications. The app is not notarised, so on first launch macOS asks for permission: right-click the app and choose Open, or allow it under System Settings > Privacy & Security.
- **Windows** (`MeteorBlitz-1.1.0-windows.zip`): unzip anywhere and run `MeteorBlitz.exe`. Windows SmartScreen may warn about an unrecognised app; choose More info > Run anyway.
- **Linux / Steam Deck** (`MeteorBlitz-1.1.0-linux-x86_64.tar.gz`): extract it and run `MeteorBlitz/meteor-blitz`. `MeteorBlitz/install.sh` adds the game to the application menu, so on a Steam Deck (in Desktop Mode) Steam's "Add a Non-Steam Game" lists it. Saves go to `~/.local/share/meteor-blitz`. This build has been tested on a Linux PC but not yet on a Steam Deck.

Settings, bindings and scores carry over from earlier versions.

The original game and assets are © Alley Labs. This is a free, non-commercial fan port.

# Meteor Blitz 1.1.1

- **No more crackle when explosions stack.** Several loud sounds at once (a bomb on a full field, a run of rock kills) were added together past full volume and clipped, which was heard as crackling. The game's sound output now has a limiter that turns only those peaks down, smoothly, and leaves everything quieter exactly as it was. At most three rock explosions now play at once, so a big blast stays an explosion rather than a wall of noise.

The web version (https://bobclawblaw.github.io/meteor-blitz/) has the same fix through the browser's own audio processing.

## Downloads

- **macOS** (`MeteorBlitz-1.1.1-macos.dmg`): universal app for Apple Silicon and Intel, macOS 11 or later. Open the DMG and drag Meteor Blitz to Applications. The app is not notarised, so on first launch macOS asks for permission: right-click the app and choose Open, or allow it under System Settings > Privacy & Security.
- **Windows** (`MeteorBlitz-1.1.1-windows.zip`): unzip anywhere and run `MeteorBlitz.exe`. Windows SmartScreen may warn about an unrecognised app; choose More info > Run anyway.
- **Linux / Steam Deck** (`MeteorBlitz-1.1.1-linux-x86_64.tar.gz`): extract it and run `MeteorBlitz/meteor-blitz`. `MeteorBlitz/install.sh` adds the game to the application menu, so on a Steam Deck (in Desktop Mode) Steam's "Add a Non-Steam Game" lists it. Saves go to `~/.local/share/meteor-blitz`. This build has been tested on a Linux PC but not yet on a Steam Deck.

Settings, bindings and scores carry over from earlier versions.

The original game and assets are © Alley Labs. This is a free, non-commercial fan port.

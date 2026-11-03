# Meteor Blitz 1.1.2

Two fixes for the Physics particle style (Options > Particle style):

- **Bosses die again.** With Physics particles on, a destroyed boss core stayed in its dying state and the stage never ended. This happened on every boss, not only the last world's.
- **No more lines across the screen.** Sparks near the edge of the wrapping play field could be drawn as a long horizontal or vertical line across the whole screen, most often on busy stages with many small rocks.

The web version (https://bobclawblaw.github.io/meteor-blitz/) has both fixes.

## Downloads

- **macOS** (`MeteorBlitz-1.1.2-macos.dmg`): universal app for Apple Silicon and Intel, macOS 11 or later. Open the DMG and drag Meteor Blitz to Applications. The app is not notarised, so on first launch macOS asks for permission: right-click the app and choose Open, or allow it under System Settings > Privacy & Security.
- **Windows** (`MeteorBlitz-1.1.2-windows.zip`): unzip anywhere and run `MeteorBlitz.exe`. Windows SmartScreen may warn about an unrecognised app; choose More info > Run anyway.
- **Linux / Steam Deck** (`MeteorBlitz-1.1.2-linux-x86_64.tar.gz`): extract it and run `MeteorBlitz/meteor-blitz`. `MeteorBlitz/install.sh` adds the game to the application menu, so on a Steam Deck (in Desktop Mode) Steam's "Add a Non-Steam Game" lists it. Saves go to `~/.local/share/meteor-blitz`. This build has been tested on a Linux PC but not yet on a Steam Deck.

Settings, bindings and scores carry over from earlier versions.

The original game and assets are © Alley Labs. This is a free, non-commercial fan port.

# Meteor Blitz 1.0.1

Fixes for 1.0.0.

- **macOS: no more crash from the menu bar.** Opening the Meteor Blitz menu in the menu bar (to quit, for example) crashed the game. Controller polling ran the macOS event loop in the middle of a frame, so the menu opened mid-frame and its redraw started a second frame inside the first. Frames are now paced by a timer, and the event loop runs between frames.
- **The results screen is no longer white or warped.** A boss's white death flash, its light and its shockwave froze at their peak when the World Complete or upgrade screen opened. Effects now finish behind the popup.
- **Esc on the main menu quits.**

## Downloads

- **macOS** (`MeteorBlitz-1.0.1-macos.dmg`): universal app for Apple Silicon and Intel, macOS 11 or later. Open the DMG and drag Meteor Blitz to Applications. The app is not notarised, so on first launch macOS asks for permission: right-click the app and choose Open, or allow it under System Settings > Privacy & Security.
- **Windows** (`MeteorBlitz-1.0.1-windows.zip`): unzip anywhere and run `MeteorBlitz.exe`. Windows SmartScreen may warn about an unrecognised app; choose More info > Run anyway.

Settings, bindings and scores from 1.0.0 carry over: they live in `~/Library/Application Support/Meteor Blitz` (macOS) or `%APPDATA%\Meteor Blitz` (Windows).

The original game and assets are © Alley Labs. This is a free, non-commercial fan port.

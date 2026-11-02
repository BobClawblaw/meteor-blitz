# Meteor Blitz 1.0.6

- **Resolution up to 4K.** Options > Resolution now sets the window and render resolution: Window, 854x480, 1280x720, 1920x1080, 2560x1440 or 3840x2160 (4K). The choice is saved in `settings.txt` and applied at launch; Window keeps the current window's fitted size. The 4K preset renders the full 3840x2160 canvas and still holds 60 fps on the test Mac.
- **Cleaner audio.** The music and effects now use cleaned copies of the original audio: resampled to 44.1 kHz, declipped, with click-free ends and a seamless music loop. This removes the crackle some systems produced.
- **Options layout.** The Options page has room for the new resolution row; all labels, values and sliders stay on screen at every supported resolution.

The web version (https://bobclawblaw.github.io/meteor-blitz/) has these changes too. In the browser, Resolution sets the render target and the canvas fills the page.

## Downloads

- **macOS** (`MeteorBlitz-1.0.6-macos.dmg`): universal app for Apple Silicon and Intel, macOS 11 or later. Open the DMG and drag Meteor Blitz to Applications. The app is not notarised, so on first launch macOS asks for permission: right-click the app and choose Open, or allow it under System Settings > Privacy & Security.
- **Windows** (`MeteorBlitz-1.0.6-windows.zip`): unzip anywhere and run `MeteorBlitz.exe`. Windows SmartScreen may warn about an unrecognised app; choose More info > Run anyway.

Settings, bindings and scores carry over from earlier versions.

The original game and assets are © Alley Labs. This is a free, non-commercial fan port.

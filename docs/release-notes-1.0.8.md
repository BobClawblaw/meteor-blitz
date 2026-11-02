# Meteor Blitz 1.0.8

- **8x art by default.** The desktop game now loads the 8x art set: sprites, UI and the bitmap font at 8x the original resolution, and planet and fog textures at 4096x4096, upscaled from the original art. The textures tile seamlessly with full detail to their edges, so the soft grid and creases some planets showed at high resolutions are gone. Large textures are sampled with mipmaps, so distant ground no longer aliases into bands.
- **World 4** uses the crater planet instead of the original city grid, whose roads read as a bright cross at high resolutions.
- **Name entry**: the letters no longer draw on black boxes, and the cursor sits on the name's line.
- **Startup** decodes the planet textures in parallel and loads each one once.

On a Mac Studio the 8x build holds 60 fps at 3840x2160 and reaches its first frame in about 2.3 seconds. It needs about 2.3 GB of graphics memory, against 0.5 GB for the previous HD set; on a graphics card with little memory, setting the environment variable `MB_HD=0` runs the original art instead.

The web version (https://bobclawblaw.github.io/meteor-blitz/) keeps the original-resolution art.

## Downloads

- **macOS** (`MeteorBlitz-1.0.8-macos.dmg`): universal app for Apple Silicon and Intel, macOS 11 or later. Open the DMG and drag Meteor Blitz to Applications. The app is not notarised, so on first launch macOS asks for permission: right-click the app and choose Open, or allow it under System Settings > Privacy & Security.
- **Windows** (`MeteorBlitz-1.0.8-windows.zip`): unzip anywhere and run `MeteorBlitz.exe`. Windows SmartScreen may warn about an unrecognised app; choose More info > Run anyway.

Settings, bindings and scores carry over from earlier versions.

The original game and assets are © Alley Labs. This is a free, non-commercial fan port.

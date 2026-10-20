#!/bin/sh
# Wraps the release build in target/MeteorBlitz.app, so macOS treats it as an
# app (GameController only routes controller input to app bundles).
# Usage: tools/make-app.sh   then   open target/MeteorBlitz.app
set -e
cd "$(dirname "$0")/.."
cargo build --release
app=target/MeteorBlitz.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp target/release/meteor-blitz "$app/Contents/MacOS/meteor-blitz"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>dev.meteorblitz.port</string>
  <key>CFBundleName</key><string>Meteor Blitz</string>
  <key>CFBundleExecutable</key><string>meteor-blitz</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>GCSupportsControllerUserInteraction</key><true/>
  <key>GCSupportedGameControllers</key>
  <array><dict><key>ProfileName</key><string>ExtendedGamepad</string></dict></array>
  <key>LSEnvironment</key>
  <dict>
    <key>MB_ROOT</key><string>$PWD</string>
    <key>MB_PAD_LOG</key><string>1</string>
  </dict>
</dict>
</plist>
PLIST
codesign --force --sign - "$app" >/dev/null 2>&1 || true
echo "$app"

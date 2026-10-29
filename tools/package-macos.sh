#!/bin/bash
# Release package for macOS: a universal (Apple Silicon + Intel) app bundle
# with the game data in Contents/Resources, wrapped in a DMG.
#
#   tools/package-macos.sh             -> dist/MeteorBlitz-<version>-macos.dmg
#   tools/package-macos.sh --app-only  -> dist/Meteor Blitz.app for this
#                                         Mac's architecture, no DMG
#
# The app is fully self-contained: the game data is in Contents/Resources
# and the binary links only system frameworks. Saves go to
# ~/Library/Application Support/Meteor Blitz (src/paths.rs).
#
# Needs both Rust targets (rustup target add aarch64-apple-darwin
# x86_64-apple-darwin). The app is signed ad hoc only: without a Developer
# ID, Gatekeeper asks the player to allow it on first launch.
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
dist=dist
app="$dist/Meteor Blitz.app"
dmg="$dist/MeteorBlitz-$version-macos.dmg"

app_only=false
[ "${1:-}" = "--app-only" ] && app_only=true

rm -rf "$app" "$dmg" "$dist/dmg"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/original/app"
if $app_only; then
  cargo build --release
  cp target/release/meteor-blitz "$app/Contents/MacOS/meteor-blitz"
else
  for target in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release --target "$target"
  done
  lipo -create \
    target/aarch64-apple-darwin/release/meteor-blitz \
    target/x86_64-apple-darwin/release/meteor-blitz \
    -output "$app/Contents/MacOS/meteor-blitz"
fi

# The data the game reads (see paths.rs): all of assets/, and from the
# original app only what is loaded at run time.
res="$app/Contents/Resources"
cp -R assets "$res/assets"
cp original/app/ngage.dat original/app/systems.dat "$res/original/app/"
cp -R original/audio original/sprites original/textures "$res/original/"
cp assets/icon/icon.icns "$res/icon.icns"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>dev.meteorblitz.port</string>
  <key>CFBundleName</key><string>Meteor Blitz</string>
  <key>CFBundleDisplayName</key><string>Meteor Blitz</string>
  <key>CFBundleExecutable</key><string>meteor-blitz</string>
  <key>CFBundleIconFile</key><string>icon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.arcade-games</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>GCSupportsControllerUserInteraction</key><true/>
  <key>GCSupportedGameControllers</key>
  <array><dict><key>ProfileName</key><string>ExtendedGamepad</string></dict></array>
</dict>
</plist>
PLIST
codesign --force --deep --sign - "$app"
if $app_only; then
  echo "$app"
  exit 0
fi

# The disk image: the app and a link to Applications to drag it into.
mkdir -p "$dist/dmg"
cp -R "$app" "$dist/dmg/"
ln -s /Applications "$dist/dmg/Applications"
hdiutil create -volname "Meteor Blitz $version" -srcfolder "$dist/dmg" -ov -format UDZO "$dmg" >/dev/null
rm -rf "$dist/dmg"
echo "$dmg"

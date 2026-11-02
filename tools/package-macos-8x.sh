#!/bin/bash
# Builds the experimental 8x-art macOS app and DMG:
#   dist/Meteor Blitz 8x.app
#   dist/MeteorBlitz-<version>-macos-8x.dmg
#
# The app is the normal 1.0.7 build with LSEnvironment MB_HD=8 and only the
# assets/8x set in Resources; the normal app keeps only assets/4x. The 8x
# set is 8x sprites/UI/font and the proven 2x terrain.
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
dist=dist
app="$dist/Meteor Blitz.app"
eight="$dist/Meteor Blitz 8x.app"
dmg="$dist/MeteorBlitz-$version-macos-8x.dmg"
stage="$dist/dmg-8x"

bash tools/package-macos.sh --app-only
rm -rf "$eight" "$dmg" "$stage"
cp -R "$app" "$eight"
# The normal app stays 4x-only; the 8x app carries only the 8x set.
rm -rf "$app/Contents/Resources/assets/8x"
rm -rf "$eight/Contents/Resources/assets/4x"
/usr/libexec/PlistBuddy \
  -c "Add :LSEnvironment dict" \
  -c "Add :LSEnvironment:MB_HD string 8" \
  "$eight/Contents/Info.plist"
codesign --force --deep --sign - "$eight"

mkdir -p "$stage"
cp -R "$eight" "$stage/"
ln -s /Applications "$stage/Applications"
hdiutil create -volname "Meteor Blitz $version 8x" -srcfolder "$stage" -ov -format UDZO "$dmg" >/dev/null
rm -rf "$stage"
echo "$eight"
echo "$dmg"

#!/bin/sh
# Builds the self-contained app bundle for this Mac (dist/Meteor Blitz.app);
# GameController only routes controller input to app bundles. The release
# DMG comes from tools/package-macos.sh without --app-only.
# Usage: tools/make-app.sh   then   open "dist/Meteor Blitz.app"
set -e
cd "$(dirname "$0")/.."
exec bash tools/package-macos.sh --app-only

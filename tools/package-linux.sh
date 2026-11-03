#!/bin/bash
# Release package for Linux (x86_64), also for the Steam Deck:
#
#   tools/package-linux.sh   -> dist/MeteorBlitz-<version>-linux-x86_64.tar.gz
#
# The binary is built in a Debian 11 container (glibc 2.31, the Steam
# Runtime's base), so it runs on that and newer distributions and on
# SteamOS. Needs Docker; DOCKER="sudo docker" if the socket needs root.
# RUST_IMAGE overrides the build image.
#
# The archive holds one folder, MeteorBlitz/: the executable, the game data
# beside it (src/paths.rs finds it there), the icon, a .desktop file and
# install.sh, which adds the game to the desktop's application menu (and so
# to Steam's "Add a Non-Steam Game" list). Saves go to
# $XDG_DATA_HOME/meteor-blitz (~/.local/share/meteor-blitz).
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
docker=${DOCKER:-docker}
image=${RUST_IMAGE:-rust:1-bullseye}
dist=dist
pkg="$dist/MeteorBlitz"
tarball="$dist/MeteorBlitz-$version-linux-x86_64.tar.gz"

# Build as the calling user so target/ stays theirs; the registry cache
# lives in target/linux-cargo so later builds skip the downloads.
$docker run --rm \
  -v "$PWD:/src" -w /src \
  -e CARGO_HOME=/src/target/linux-cargo \
  -e CARGO_TARGET_DIR=/src/target/linux \
  "$image" bash -c "
    set -e
    # Debian 11 is past its LTS and its mirrors drift (the security index
    # listed packages its pool no longer had), so use the snapshot the
    # image was built from: the same packages every time.
    sed -n 's/^# *\(deb http:\/\/snapshot\.debian\.org\)/\1/p' /etc/apt/sources.list.d/* /etc/apt/sources.list 2>/dev/null > /tmp/snapshot.list || true
    if [ -s /tmp/snapshot.list ]; then
      rm -f /etc/apt/sources.list.d/*
      mv /tmp/snapshot.list /etc/apt/sources.list
    fi
    apt-get -o Acquire::Check-Valid-Until=false update -qq
    apt-get install -y -qq --no-install-recommends \
      pkg-config libasound2-dev libudev-dev libx11-dev libxi-dev libgl1-mesa-dev >/dev/null
    git config --global --add safe.directory /src   # build.rs dates the build from git
    cargo build --release --locked
    chown -R $(id -u):$(id -g) target/linux target/linux-cargo
  "

rm -rf "$pkg" "$tarball"
mkdir -p "$pkg/original/app"
cp target/linux/release/meteor-blitz "$pkg/meteor-blitz"
strip "$pkg/meteor-blitz" 2>/dev/null || true

# The data the game reads (see paths.rs), as in the other packages.
cp -R assets "$pkg/assets"
rm -rf "$pkg/assets/web-audio"   # the web build's MP3s; desktop loads assets/audio
rm -rf "$pkg/assets/4x"          # the package loads the 8x set
cp original/app/ngage.dat original/app/systems.dat "$pkg/original/app/"
cp -R original/sprites original/textures "$pkg/original/"
cp assets/icon/icon-1024.png "$pkg/meteor-blitz.png"

cat > "$pkg/meteor-blitz.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Meteor Blitz
Comment=Twin-stick arcade shooter
Exec=meteor-blitz
Icon=meteor-blitz
Terminal=false
Categories=Game;ArcadeGame;
DESKTOP

cat > "$pkg/install.sh" <<'INSTALL'
#!/bin/bash
# Adds Meteor Blitz to the application menu (and Steam's non-Steam game
# list), running it from this folder. Run it again after moving the folder;
# remove ~/.local/share/applications/meteor-blitz.desktop to undo.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
mkdir -p "$apps"
sed -e "s|^Exec=.*|Exec=\"$here/meteor-blitz\"|" \
    -e "s|^Icon=.*|Icon=$here/meteor-blitz.png|" \
    "$here/meteor-blitz.desktop" > "$apps/meteor-blitz.desktop"
chmod +x "$here/meteor-blitz"
command -v update-desktop-database >/dev/null && update-desktop-database "$apps" || true
echo "Added Meteor Blitz to the application menu."
INSTALL
chmod +x "$pkg/install.sh" "$pkg/meteor-blitz"

tar -C "$dist" -czf "$tarball" MeteorBlitz
rm -rf "$pkg"
echo "$tarball"

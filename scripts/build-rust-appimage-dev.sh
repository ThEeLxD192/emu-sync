#!/usr/bin/env bash
# Development-only build. Graphics drivers remain host dependencies; do not release.
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "Usage: $0 <new-output.AppImage>" >&2
    exit 2
fi
if [[ $(uname -s) != Linux || $(uname -m) != x86_64 ]]; then
    echo "This development build requires x86_64 Linux." >&2
    exit 1
fi
output=$1
if [[ -e "$output" ]]; then
    echo "Refusing to replace existing output: $output" >&2
    exit 1
fi
if [[ ! -d $(dirname "$output") ]]; then
    echo "Output directory does not exist: $(dirname "$output")" >&2
    exit 1
fi
appimagetool=${APPIMAGETOOL:-appimagetool}
if ! command -v "$appimagetool" >/dev/null; then
    echo "appimagetool is required; set APPIMAGETOOL to its executable path." >&2
    exit 1
fi
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
output=$(cd -- "$(dirname -- "$output")" && pwd)/$(basename -- "$output")
tmp=$(mktemp -d "$(dirname -- "$output")/.emusync-build.XXXXXXXX")
trap 'rm -rf -- "$tmp"' EXIT
appdir=$tmp/EmuSync.AppDir
app_id=io.github.ThEeLxD192.EmuSync
mkdir -p "$appdir/usr/bin" "$appdir/usr/share/metainfo" "$appdir/usr/share/applications" "$appdir/usr/share/pixmaps"
cd "$repo"
if [[ -n ${EMUSYNC_RUST_BINARY:-} ]]; then
    binary=$EMUSYNC_RUST_BINARY
else
    cargo build --release --locked -p emusync-desktop-app
    binary=target/release/emusync-desktop-app
fi
install -m 755 "$binary" "$appdir/usr/bin/EmuSync"
if [[ -n ${EMUSYNC_RUST_RUNTIME_DIR:-} ]]; then
    cp -a "$EMUSYNC_RUST_RUNTIME_DIR/usr/bin/." "$appdir/usr/bin/"
    cp -a "$EMUSYNC_RUST_RUNTIME_DIR/usr/lib" "$appdir/usr/"
    cp -a "$EMUSYNC_RUST_RUNTIME_DIR/usr/share/doc" "$appdir/usr/share/"
    cp -a "$EMUSYNC_RUST_RUNTIME_DIR/usr/share/emusync" "$appdir/usr/share/"
fi
install -m 644 desktopApp/src/main/resources/icon.png "$appdir/EmuSync.png"
install -m 644 desktopApp/src/main/resources/icon.png "$appdir/usr/share/pixmaps/EmuSync.png"
install -m 644 "desktopApp/src/main/resources/$app_id.appdata.xml" "$appdir/usr/share/metainfo/$app_id.appdata.xml"
install -m 755 packaging/linux/AppRun "$appdir/AppRun"
cat > "$appdir/$app_id.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=EmuSync
Comment=Centralized game launcher and save sync manager
Exec=EmuSync
Icon=EmuSync
Categories=Game;
Terminal=false
DESKTOP
cp "$appdir/$app_id.desktop" "$appdir/usr/share/applications/"
appstreamcli validate --no-net "$appdir/usr/share/metainfo/$app_id.appdata.xml"
ARCH=x86_64 "$appimagetool" "$appdir" "$tmp/EmuSync-rust-dev-x86_64.AppImage"
# Avoid overwriting any artifact created while the build was running.
ln "$tmp/EmuSync-rust-dev-x86_64.AppImage" "$output"
chmod 755 "$output"
echo "Development AppImage: $output"
echo "Host graphics drivers and system libraries are required; this artifact is not a release."

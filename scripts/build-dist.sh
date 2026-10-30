#!/bin/bash
set -e

echo "=========================================="
echo "  1. Compiling EmuSync with Gradle...     "
echo "=========================================="
./gradlew packageReleaseAppImage --no-daemon

echo "=========================================="
echo "  2. Generating EmuSync AppImage...       "
echo "=========================================="
APP_DIR="/tmp/EmuSync.AppDir"
APP_ID="io.github.ThEeLxD192.EmuSync"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR"

# Copy Compose Desktop runtime and application binaries
cp -r build/compose/binaries/main-release/app/EmuSync/* "$APP_DIR/"

# Create AppRun bootstrap script
cat << 'EOF' > "$APP_DIR/AppRun"
#!/bin/sh
HERE="$(dirname "$(readlink -f "${0}")")"
exec "$HERE/bin/EmuSync" "$@"
EOF
chmod +x "$APP_DIR/AppRun"

# Copy application icons and desktop metadata
cp src/main/resources/icon.png "$APP_DIR/icon.png"
cp src/main/resources/icon.png "$APP_DIR/EmuSync.png"
cat << 'EOF' > "$APP_DIR/$APP_ID.desktop"
[Desktop Entry]
Type=Application
Name=EmuSync
Comment=Centralized game launcher and save sync manager
Exec=EmuSync
Icon=EmuSync
Categories=Game;
Terminal=false
EOF
chmod +x "$APP_DIR/$APP_ID.desktop"

# Include AppStream metadata and the desktop entry in their standard locations.
mkdir -p "$APP_DIR/usr/share/metainfo" "$APP_DIR/usr/share/applications" \
    "$APP_DIR/usr/share/pixmaps"
cp "src/main/resources/$APP_ID.appdata.xml" "$APP_DIR/usr/share/metainfo/"
cp "$APP_DIR/$APP_ID.desktop" "$APP_DIR/usr/share/applications/"
cp src/main/resources/icon.png "$APP_DIR/usr/share/pixmaps/EmuSync.png"
appstreamcli validate --no-net "$APP_DIR/usr/share/metainfo/$APP_ID.appdata.xml"

# Package AppImage with appimagetool into /out (mapped to root directory)
TARGET_APPIMAGE="/out/EmuSync-x86_64.AppImage"
rm -f "$TARGET_APPIMAGE" "/out/EmuSync-1.0.0-x86_64.AppImage" 2>/dev/null || true
ARCH=x86_64 appimagetool "$APP_DIR" "$TARGET_APPIMAGE"

# Ensure executable permissions
chmod 755 "$TARGET_APPIMAGE" 2>/dev/null || true

echo "=========================================="
echo "  Done! AppImage generated in project root:"
echo "  - EmuSync-x86_64.AppImage               "
echo "=========================================="

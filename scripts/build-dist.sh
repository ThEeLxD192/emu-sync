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
cat << 'EOF' > "$APP_DIR/EmuSync.desktop"
[Desktop Entry]
Type=Application
Name=EmuSync
Comment=Centralized game launcher and save sync manager
Exec=EmuSync
Icon=EmuSync
Categories=Game;
Terminal=false
EOF
chmod +x "$APP_DIR/EmuSync.desktop"

# Package AppImage with appimagetool into /out (mapped to root directory)
ARCH=x86_64 appimagetool "$APP_DIR" "/out/EmuSync-1.0.0-x86_64.AppImage"

# Ensure executable permissions
chmod 755 /out/EmuSync-1.0.0-x86_64.AppImage 2>/dev/null || true

echo "=========================================="
echo "  Done! AppImage generated in project root:"
echo "  - EmuSync-1.0.0-x86_64.AppImage         "
echo "=========================================="

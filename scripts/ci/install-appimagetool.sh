#!/usr/bin/env bash
# Downloads appimagetool and extracts it so it runs without FUSE on CI runners.
set -euo pipefail
if [[ $# -ne 1 ]]; then
    echo "Usage: $0 <new-directory>" >&2
    exit 2
fi
destination=$1
if [[ -e "$destination" ]]; then
    echo "Refusing to replace existing path: $destination" >&2
    exit 1
fi
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
curl -fsSL -o "$tmp/appimagetool.AppImage" \
    https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
chmod +x "$tmp/appimagetool.AppImage"
(cd "$tmp" && ./appimagetool.AppImage --appimage-extract >/dev/null)
mv "$tmp/squashfs-root" "$destination"
echo "appimagetool: $destination/AppRun"

#!/usr/bin/env bash
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
appdir="$tmp/App Dir"
mkdir -p "$appdir/usr/bin" "$appdir/usr/share/emusync"
mkdir -p "$appdir/usr/lib/emusync-fallback/libmissing.so"
: > "$appdir/usr/lib/emusync-fallback/libmissing.so/libmissing.so"
cp "$repo/packaging/linux/AppRun" "$appdir/AppRun"
printf '%s\n' libavailable.so libmissing.so > "$appdir/usr/share/emusync/fallback-libraries.txt"
cat > "$appdir/usr/bin/emusync-probe-library" <<'PROBE'
#!/bin/sh
case "$1" in
    libavailable.so) exit 0 ;;
    libmissing.so) case ":${LD_LIBRARY_PATH-}:" in */usr/lib/emusync-fallback/libmissing.so:*) exit 0 ;; esac ;;
esac
exit 1
PROBE
cat > "$appdir/usr/bin/EmuSync" <<'APP'
#!/bin/sh
printf '%s\n%s\n%s\n' "$APPDIR" "${LD_LIBRARY_PATH-unset}" "$1" > "$EMUSYNC_TEST_OUTPUT"
APP
chmod +x "$appdir/AppRun" "$appdir/usr/bin/emusync-probe-library" "$appdir/usr/bin/EmuSync"
fallback="$appdir/usr/lib/emusync-fallback/libmissing.so"
for mode in unset existing; do
    output="$tmp/$mode"
    if [[ $mode == unset ]]; then
        env -u LD_LIBRARY_PATH EMUSYNC_TEST_OUTPUT="$output" "$appdir/AppRun" 'argument with spaces'
        expected=$fallback
    else
        env LD_LIBRARY_PATH=/external/lib EMUSYNC_TEST_OUTPUT="$output" "$appdir/AppRun" 'argument with spaces'
        expected="/external/lib:$fallback"
    fi
    printf '%s\n%s\n%s\n' "$appdir" "$expected" 'argument with spaces' > "$tmp/expected"
    cmp "$tmp/expected" "$output"
done
rm "$appdir/usr/lib/emusync-fallback/libmissing.so/libmissing.so"
if env -u LD_LIBRARY_PATH EMUSYNC_TEST_OUTPUT="$tmp/should-not-start" \
    "$appdir/AppRun" ignored > /dev/null 2>&1; then
    echo "Missing fallback library unexpectedly accepted" >&2
    exit 1
fi
test ! -e "$tmp/should-not-start"
echo "AppImage fallback selection checks passed"

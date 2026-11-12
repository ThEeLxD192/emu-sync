#!/usr/bin/env bash
# Builds the Windows MSI installer (WiX 4) and the portable ZIP on native Windows with Git Bash.
set -euo pipefail
if [[ $# -ne 1 ]]; then
    echo "Usage: $0 <existing-output-directory>" >&2
    exit 2
fi
case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) ;;
    *) echo "Native Windows with Git Bash is required." >&2; exit 1 ;;
esac
for tool in cargo rustc wix cygpath powershell; do
    command -v "$tool" >/dev/null || { echo "Required tool not found: $tool" >&2; exit 1; }
done
wix_version=$(wix --version)
if [[ ! $wix_version =~ ^4\. ]]; then
    echo "This script requires WiX 4 and its UI extension (found $wix_version)." >&2
    exit 1
fi
output=$1
if [[ ! -d $output ]]; then
    echo "Output directory does not exist: $output" >&2
    exit 1
fi
output=$(cd -- "$output" && pwd)
for name in EmuSync-x86_64.msi EmuSync-x86_64-portable.zip; do
    if [[ -e "$output/$name" ]]; then
        echo "Refusing to replace existing output: $output/$name" >&2
        exit 1
    fi
done
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo"
version=$(awk '/^\[/ { section = ($0 == "[workspace.package]") } section && /^version = / { split($0,p,"\""); print p[2] }' Cargo.toml)
if [[ ! $version =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
    echo "Invalid Cargo workspace version: $version" >&2
    exit 1
fi
IFS=. read -r major minor patch <<< "$version"
if (( major > 255 || minor > 255 || patch > 65535 )); then
    echo "Version exceeds Windows Installer limits: $version" >&2
    exit 1
fi
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT

# The installer wizard shows the license as RTF; the repository keeps it as plain text.
{
    printf '{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0 Courier New;}}\\f0\\fs20 '
    sed -e 's/\\/\\\\/g' -e 's/{/\\{/g' -e 's/}/\\}/g' -e 's/\r$//' -e 's/$/\\par/' LICENSE
    printf '}'
} > "$tmp/license.rtf"

cargo build --release --locked --target x86_64-pc-windows-msvc -p emusync-desktop-app
built="$repo/target/x86_64-pc-windows-msvc/release/emusync-desktop-app.exe"

wix build -arch x64 -ext WixToolset.UI.wixext \
    -d "Version=$version" \
    -d "Executable=$(cygpath -w "$built")" \
    -d "IconFile=$(cygpath -w "$repo/packaging/assets/icon.ico")" \
    -d "LicenseRtf=$(cygpath -w "$tmp/license.rtf")" \
    -o "$(cygpath -w "$tmp/EmuSync-x86_64.msi")" packaging/windows/product.wxs

# Portable layout: the marker file next to the executable keeps data beside it and turns off
# the installer-based updater, which would otherwise install the machine-wide MSI.
stage="$tmp/portable/EmuSync"
mkdir -p "$stage"
cp "$built" "$stage/EmuSync.exe"
cp LICENSE "$stage/LICENSE"
printf '%s\r\n' \
    'This file marks EmuSync as portable.' \
    'Its settings, history and backups are kept in this folder, and the application' \
    'does not install updates by itself. Delete this file to use the standard locations.' \
    > "$stage/portable.txt"
# Compress-Archive in Windows PowerShell 5.1 stores backslash paths, which other extractors
# read wrongly; ZipFile writes standard forward-slash entries.
powershell -NoProfile -Command \
    "Add-Type -AssemblyName System.IO.Compression.FileSystem; [System.IO.Compression.ZipFile]::CreateFromDirectory('$(cygpath -w "$stage")', '$(cygpath -w "$tmp/EmuSync-x86_64-portable.zip")', [System.IO.Compression.CompressionLevel]::Optimal, \$true)"

# noclobber prevents replacing output even if another process creates it during the build.
(set -C; cat "$tmp/EmuSync-x86_64.msi" > "$output/EmuSync-x86_64.msi")
(set -C; cat "$tmp/EmuSync-x86_64-portable.zip" > "$output/EmuSync-x86_64-portable.zip")
echo "Windows artifacts written to $output"

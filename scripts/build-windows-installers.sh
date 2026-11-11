#!/usr/bin/env bash
# Windows MSI/EXE packaging with WiX 4. Draft: native installation and upgrade are not yet validated.
set -euo pipefail
if [[ $# -ne 2 ]]; then
    echo "Usage: $0 <license.rtf> <existing-output-directory>" >&2
    exit 2
fi
case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) ;;
    *) echo "Native Windows with Git Bash is required." >&2; exit 1 ;;
esac
for tool in cargo rustc wix cygpath; do
    command -v "$tool" >/dev/null || { echo "Required tool not found: $tool" >&2; exit 1; }
done
wix_version=$(wix --version)
if [[ ! $wix_version =~ ^4\. ]]; then
    echo "This draft requires WiX 4 and its UI and Bal extensions." >&2
    exit 1
fi
license=$1
output=$2
if [[ ! -f $license || $(head -c 5 "$license") != '{\rtf' ]]; then
    echo "Provide an RTF license file appropriate for the project." >&2
    exit 1
fi
if [[ ! -d $output ]]; then
    echo "Output directory does not exist: $output" >&2
    exit 1
fi
output=$(cd -- "$output" && pwd)
for name in EmuSync-x86_64.msi EmuSync-x86_64.exe; do
    if [[ -e "$output/$name" ]]; then
        echo "Refusing to replace existing output: $output/$name" >&2
        exit 1
    fi
done
license=$(cygpath -w "$(cd -- "$(dirname -- "$license")" && pwd)/$(basename -- "$license")")
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
cargo build --release --locked --target x86_64-pc-windows-msvc -p emusync-desktop-app
executable=$(cygpath -w "$repo/target/x86_64-pc-windows-msvc/release/emusync-desktop-app.exe")
icon=$(cygpath -w "$repo/packaging/assets/icon.ico")
msi=$(cygpath -w "$tmp/EmuSync-x86_64.msi")
exe=$(cygpath -w "$tmp/EmuSync-x86_64.exe")
wix build -arch x64 -ext WixToolset.UI.wixext \
    -d "Version=$version" -d "Executable=$executable" -d "IconFile=$icon" -d "LicenseRtf=$license" \
    -o "$msi" packaging/windows/product.wxs
wix build -arch x64 -ext WixToolset.Bal.wixext \
    -d "Version=$version" -d "MsiFile=$msi" -d "IconFile=$icon" \
    -o "$exe" packaging/windows/bundle.wxs
# noclobber prevents replacing output even if another process creates it during the build.
(set -C; cat "$tmp/EmuSync-x86_64.msi" > "$output/EmuSync-x86_64.msi")
(set -C; cat "$tmp/EmuSync-x86_64.exe" > "$output/EmuSync-x86_64.exe")
echo "Draft artifacts written to $output; native installation and upgrade validation remain pending."

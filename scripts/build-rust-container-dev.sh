#!/usr/bin/env bash
# Build a development-only AppImage on a Debian Bookworm baseline.
set -euo pipefail
if [[ $# -ne 1 ]]; then
    echo "Usage: $0 <new-output.AppImage>" >&2
    exit 2
fi
output=$1
if [[ -e "$output" || ! -d $(dirname -- "$output") ]]; then
    echo "Output must be a new file in an existing directory: $output" >&2
    exit 1
fi
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
output=$(cd -- "$(dirname -- "$output")" && pwd)/$(basename -- "$output")
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
cd "$repo"
# Include tracked and non-ignored source changes, never configuration or build data.
git ls-files -z --cached --others --exclude-standard -- \
    Dockerfile.rust-dev Cargo.toml Cargo.lock rust-toolchain.toml rust xtask packaging/linux |
    tar --null -T - -cf - | docker build -f Dockerfile.rust-dev -t emusync-rust-dev-builder:local -
docker run --rm --entrypoint cat emusync-rust-dev-builder:local /usr/local/bin/EmuSync > "$tmp/EmuSync"
chmod 755 "$tmp/EmuSync"
mkdir -p "$tmp/runtime"
docker run --rm --entrypoint tar emusync-rust-dev-builder:local -C /opt/emusync-runtime -cf - . |
    tar -C "$tmp/runtime" -xf -
EMUSYNC_RUST_BINARY="$tmp/EmuSync" EMUSYNC_RUST_RUNTIME_DIR="$tmp/runtime" \
    "$repo/scripts/build-rust-appimage-dev.sh" "$output"

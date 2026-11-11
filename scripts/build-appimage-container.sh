#!/usr/bin/env bash
# Builds the Linux AppImage entirely in Docker (Debian Bookworm); the host needs only Docker and Git.
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
tmp=$(mktemp -d "$(dirname -- "$output")/.emusync-build.XXXXXXXX")
trap 'rm -rf -- "$tmp"' EXIT
cd "$repo"
build_args=()
if [[ -n ${CARGO_BUILD_JOBS:-} ]]; then
    build_args=(--build-arg "CARGO_BUILD_JOBS=$CARGO_BUILD_JOBS")
fi
# Include tracked and non-ignored source changes, never configuration or build data.
git ls-files -z --cached --others --exclude-standard -- \
    Dockerfile Cargo.toml Cargo.lock rust-toolchain.toml rust xtask packaging scripts/build-appimage.sh |
    tar --null -T - -cf - | docker build "${build_args[@]}" -f Dockerfile -t emusync-builder:local -
docker run --rm --entrypoint cat emusync-builder:local /out/EmuSync-x86_64.AppImage > "$tmp/EmuSync-x86_64.AppImage"
chmod 755 "$tmp/EmuSync-x86_64.AppImage"
# Avoid overwriting any artifact created while the build was running.
ln "$tmp/EmuSync-x86_64.AppImage" "$output"
echo "AppImage: $output"

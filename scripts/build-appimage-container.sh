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
build_args=(--output "type=local,dest=$tmp")
if [[ -n ${CARGO_BUILD_JOBS:-} ]]; then
    build_args+=(--build-arg "CARGO_BUILD_JOBS=$CARGO_BUILD_JOBS")
fi
# Optional BuildKit layer cache, e.g. `type=gha,scope=appimage` in GitHub Actions.
if [[ -n ${EMUSYNC_DOCKER_CACHE_FROM:-} ]]; then
    build_args+=(--cache-from "$EMUSYNC_DOCKER_CACHE_FROM")
fi
if [[ -n ${EMUSYNC_DOCKER_CACHE_TO:-} ]]; then
    build_args+=(--cache-to "$EMUSYNC_DOCKER_CACHE_TO")
fi
# Include tracked and non-ignored source changes, never configuration or build data.
git ls-files -z --cached --others --exclude-standard -- \
    Dockerfile Cargo.toml Cargo.lock rust-toolchain.toml rust xtask packaging scripts/build-appimage.sh |
    tar --null -T - -cf - | docker build "${build_args[@]}" -f Dockerfile -
chmod 755 "$tmp/EmuSync-x86_64.AppImage"
# Avoid overwriting any artifact created while the build was running.
ln "$tmp/EmuSync-x86_64.AppImage" "$output"
echo "AppImage: $output"

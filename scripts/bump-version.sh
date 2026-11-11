#!/usr/bin/env bash
set -euo pipefail

bump_type=${1:-patch}
cargo_file=Cargo.toml
lock_file=Cargo.lock

# Fail before modifying anything if versions drift or a declaration is ambiguous.
cargo_version=$(awk '
    /^\[/ { section = ($0 == "[workspace.package]") }
    section && /^version = "[^"]+"$/ { split($0, p, "\""); v=p[2]; n++ }
    END { if (n != 1) exit 1; print v }
' "$cargo_file") || { echo "Error: Cargo.toml must declare one workspace version." >&2; exit 1; }
if [[ ! $cargo_version =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
    echo "Error: the Cargo workspace version must be a three-part SemVer." >&2
    exit 1
fi
awk -v expected="$cargo_version" '
    /^\[\[package\]\]$/ { package="" }
    /^name = "/ { split($0, p, "\""); package=p[2] }
    /^version = "/ && (package == "emusync-desktop-app" || package == "emusync-presentation") {
        split($0, p, "\""); if (p[2] != expected) exit 1; seen[package]++
    }
    END { if (seen["emusync-desktop-app"] != 1 || seen["emusync-presentation"] != 1) exit 1 }
' "$lock_file" || { echo "Error: Cargo.lock application versions do not match." >&2; exit 1; }

IFS=. read -r major minor patch <<< "$cargo_version"
case "$bump_type" in
    patch|fix) patch=$((patch + 1)) ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    major) major=$((major + 1)); minor=0; patch=0 ;;
    *) echo "Unknown bump type: $bump_type. Use patch, minor, or major." >&2; exit 1 ;;
esac
new_version="$major.$minor.$patch"
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT

awk -v v="$new_version" '/^\[/ { section = ($0 == "[workspace.package]") } section && /^version = "[^"]+"$/ { sub(/"[^"]+"/, "\"" v "\"") } { print }' "$cargo_file" > "$tmp/cargo"
awk -v v="$new_version" '
    /^\[\[package\]\]$/ { package="" }
    /^name = "/ { split($0, p, "\""); package=p[2] }
    /^version = "/ && (package == "emusync-desktop-app" || package == "emusync-presentation") { sub(/"[^"]+"/, "\"" v "\"") }
    { print }
' "$lock_file" > "$tmp/lock"

# Preserve the checked-in files' permissions on both Linux and Git Bash.
cat "$tmp/cargo" > "$cargo_file"
cat "$tmp/lock" > "$lock_file"
echo "Bumping version: $cargo_version -> $new_version ($bump_type)"
if [[ -n ${GITHUB_OUTPUT:-} ]]; then
    printf 'new_version=%s\ntag_name=v%s\n' "$new_version" "$new_version" >> "$GITHUB_OUTPUT"
fi

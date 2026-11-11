#!/usr/bin/env bash
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT

fixture() {
    local dir=$1
    mkdir -p "$dir/scripts"
    cp "$repo/Cargo.toml" "$repo/Cargo.lock" "$dir/"
    cp "$repo/scripts/bump-version.sh" "$dir/scripts/"
}

version=$(awk '/^\[/ { section = ($0 == "[workspace.package]") } section && /^version = / { split($0,p,"\""); print p[2] }' "$repo/Cargo.toml")
IFS=. read -r major minor patch <<< "$version"
for mode in patch minor major; do
    case "$mode" in
        patch) expected="$major.$minor.$((patch + 1))" ;;
        minor) expected="$major.$((minor + 1)).0" ;;
        major) expected="$((major + 1)).0.0" ;;
    esac
    dir="$tmp/$mode"
    fixture "$dir"
    (cd "$dir" && GITHUB_OUTPUT="$dir/output" bash scripts/bump-version.sh "$mode")
    grep -Fxq "version = \"$expected\"" "$dir/Cargo.toml"
    awk -v expected="$expected" '
        /^name = "/ { split($0,p,"\""); name=p[2] }
        /^version = "/ && (name == "emusync-desktop-app" || name == "emusync-presentation") {
            split($0,p,"\""); if (p[2] != expected) exit 1; count++
        }
        END { if (count != 2) exit 1 }
    ' "$dir/Cargo.lock"
    grep -Fxq "new_version=$expected" "$dir/output"
    grep -Fxq "tag_name=v$expected" "$dir/output"
done

dir="$tmp/ambiguous"
fixture "$dir"
sed -i '/^\[workspace.package\]$/a version = "999999.999999.999999"' "$dir/Cargo.toml"
cp "$dir/Cargo.toml" "$dir/cargo-before"
cp "$dir/Cargo.lock" "$dir/lock-before"
if (cd "$dir" && bash scripts/bump-version.sh patch >/dev/null 2>&1); then
    echo "Ambiguous workspace version unexpectedly accepted" >&2
    exit 1
fi
cmp "$dir/Cargo.toml" "$dir/cargo-before"
cmp "$dir/Cargo.lock" "$dir/lock-before"

dir="$tmp/lock-mismatch"
fixture "$dir"
sed -i '/name = "emusync-desktop-app"/{n;s/^version = .*/version = "999999.999999.999999"/;}' "$dir/Cargo.lock"
cp "$dir/Cargo.toml" "$dir/cargo-before"
cp "$dir/Cargo.lock" "$dir/lock-before"
if (cd "$dir" && bash scripts/bump-version.sh patch >/dev/null 2>&1); then
    echo "Mismatched Cargo.lock unexpectedly accepted" >&2
    exit 1
fi
cmp "$dir/Cargo.toml" "$dir/cargo-before"
cmp "$dir/Cargo.lock" "$dir/lock-before"

dir="$tmp/invalid"
fixture "$dir"
cp "$dir/Cargo.toml" "$dir/cargo-before"
if (cd "$dir" && bash scripts/bump-version.sh invalid >/dev/null 2>&1); then
    echo "Unknown version bump unexpectedly accepted" >&2
    exit 1
fi
cmp "$dir/Cargo.toml" "$dir/cargo-before"
echo "Version bump checks passed"

#!/usr/bin/env bash
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT

fixture() {
    local dir=$1
    mkdir -p "$dir/desktopApp/src/main/kotlin/com/emusync" "$dir/scripts"
    cp "$repo/desktopApp/src/main/kotlin/com/emusync/AppInfo.kt" "$dir/desktopApp/src/main/kotlin/com/emusync/AppInfo.kt"
    cp "$repo/desktopApp/build.gradle.kts" "$dir/desktopApp/build.gradle.kts"
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
    grep -Fxq "    const val VERSION = \"$expected\"" "$dir/desktopApp/src/main/kotlin/com/emusync/AppInfo.kt"
    grep -Fxq "            packageVersion = \"$expected\"" "$dir/desktopApp/build.gradle.kts"
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

dir="$tmp/mismatch"
fixture "$dir"
sed -i -E 's/(packageVersion = ")[^"]+"/\1999999.999999.999999"/' "$dir/desktopApp/build.gradle.kts"
cp "$dir/Cargo.toml" "$dir/cargo-before"
cp "$dir/Cargo.lock" "$dir/lock-before"
cp "$dir/desktopApp/src/main/kotlin/com/emusync/AppInfo.kt" "$dir/app-before"
if (cd "$dir" && bash scripts/bump-version.sh patch >/dev/null 2>&1); then
    echo "Mismatched versions unexpectedly accepted" >&2
    exit 1
fi
cmp "$dir/Cargo.toml" "$dir/cargo-before"
cmp "$dir/Cargo.lock" "$dir/lock-before"
cmp "$dir/desktopApp/src/main/kotlin/com/emusync/AppInfo.kt" "$dir/app-before"

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

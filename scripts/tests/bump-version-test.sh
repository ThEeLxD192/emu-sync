#!/usr/bin/env bash
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT

fixture() {
    local dir=$1
    mkdir -p "$dir/scripts" "$dir/xtask"
    cp "$repo/Cargo.toml" "$repo/Cargo.lock" "$dir/"
    cp "$repo/scripts/bump-version.sh" "$dir/scripts/"
    cp "$repo/xtask/Cargo.toml" "$dir/xtask/"
    local manifest
    for manifest in "$repo"/rust/*/Cargo.toml; do
        mkdir -p "$dir/rust/$(basename "$(dirname "$manifest")")"
        cp "$manifest" "$dir/rust/$(basename "$(dirname "$manifest")")/"
    done
}

# Fails unless every Cargo.lock entry of a package inheriting the workspace version
# (its manifest has `version.workspace = true`) is at the expected version.
assert_lock_versions() {
    local dir=$1 expected=$2 names
    names=$(grep -lx 'version.workspace = true' "$dir"/rust/*/Cargo.toml "$dir/xtask/Cargo.toml" |
        xargs -n1 awk -F'"' '/^name = /{ print $2; exit }' | tr '\n' ' ')
    [[ -n $names ]]
    awk -v expected="$expected" -v names="$names" '
        BEGIN { n = split(names, list, " "); for (i = 1; i <= n; i++) wanted[list[i]] = 1 }
        /^name = "/ { split($0,p,"\""); name=p[2] }
        /^version = "/ && (name in wanted) {
            split($0,p,"\""); if (p[2] != expected) exit 1; seen[name]++
        }
        END { for (name in wanted) if (seen[name] != 1) exit 1 }
    ' "$dir/Cargo.lock"
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
    assert_lock_versions "$dir" "$expected"
    # The application, desktop app and presentation crates follow the workspace version.
    for name in emusync-application emusync-desktop-app emusync-presentation; do
        grep -A1 -Fx "name = \"$name\"" "$dir/Cargo.lock" | grep -Fxq "version = \"$expected\""
    done
    grep -Fxq "new_version=$expected" "$dir/output"
    grep -Fxq "tag_name=v$expected" "$dir/output"
done

# A crate that starts inheriting the workspace version is bumped too.
dir="$tmp/new-inheritor"
fixture "$dir"
sed -i 's/^version = "[^"]*"$/version.workspace = true/' "$dir/rust/domain/Cargo.toml"
sed -i '/^name = "emusync-domain"$/{n;s/^version = .*/version = "'"$version"'"/;}' "$dir/Cargo.lock"
(cd "$dir" && bash scripts/bump-version.sh patch >/dev/null)
assert_lock_versions "$dir" "$major.$minor.$((patch + 1))"
grep -A1 -Fx 'name = "emusync-domain"' "$dir/Cargo.lock" | grep -Fxq "version = \"$major.$minor.$((patch + 1))\""

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
for name in emusync-desktop-app emusync-application; do
    fixture "$dir"
    sed -i '/^name = "'"$name"'"$/{n;s/^version = .*/version = "999999.999999.999999"/;}' "$dir/Cargo.lock"
    cp "$dir/Cargo.toml" "$dir/cargo-before"
    cp "$dir/Cargo.lock" "$dir/lock-before"
    if (cd "$dir" && bash scripts/bump-version.sh patch >/dev/null 2>&1); then
        echo "Mismatched Cargo.lock entry for $name unexpectedly accepted" >&2
        exit 1
    fi
    cmp "$dir/Cargo.toml" "$dir/cargo-before"
    cmp "$dir/Cargo.lock" "$dir/lock-before"
    rm -rf "$dir"
done

dir="$tmp/invalid"
fixture "$dir"
cp "$dir/Cargo.toml" "$dir/cargo-before"
if (cd "$dir" && bash scripts/bump-version.sh invalid >/dev/null 2>&1); then
    echo "Unknown version bump unexpectedly accepted" >&2
    exit 1
fi
cmp "$dir/Cargo.toml" "$dir/cargo-before"
echo "Version bump checks passed"

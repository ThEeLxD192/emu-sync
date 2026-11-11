#!/usr/bin/env bash
# Exercise index isolation and installation in disposable repositories; never touch the user index.
set -euo pipefail

source_root=$(cd "$(dirname "$0")/../.." && pwd)
test_root=$(mktemp -d "${TMPDIR:-/tmp}/emusync-hook-tests.XXXXXXXX")
trap 'cd "$source_root"; rm -rf -- "$test_root"' EXIT
export GIT_CONFIG_NOSYSTEM=1
export GIT_CONFIG_GLOBAL=/dev/null
export EMUSYNC_HOOK_TEST_LOG="$test_root/invocations"
export EMUSYNC_HOOK_TEST_SNAPSHOT="$test_root/snapshot-path"
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE

# A fake Cargo records each call and fails when the snapshot holds bad staged content.
mkdir -p "$test_root/bin"
cat > "$test_root/bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "$*" == 'xtask quality' ]]
printf 'cargo\n' >> "$EMUSYNC_HOOK_TEST_LOG"
pwd > "$EMUSYNC_HOOK_TEST_SNAPSHOT"
if [[ ${EMUSYNC_HOOK_TEST_INTERRUPT:-0} == 1 ]]; then
    kill -TERM "$PPID"
    exit 0
fi
! grep -rqsx BAD rust
EOF
chmod +x "$test_root/bin/cargo"
export PATH="$test_root/bin:$PATH"

mkdir -p "$test_root/repo with spaces"
cd "$test_root/repo with spaces"
git init --quiet
mkdir -p .githooks scripts rust/domain/src
cp "$source_root/.githooks/pre-commit" .githooks/
cp "$source_root/scripts/install-git-hooks.sh" scripts/
printf 'lock\n' > Cargo.lock
printf 'INITIAL\n' > 'rust/domain/src/file with spaces.rs'
git add Cargo.lock 'rust/domain/src/file with spaces.rs'
fixture_tree=$(git write-tree)
fixture_commit=$(git -c user.name=HookTest -c user.email=hook-test@example.invalid commit-tree "$fixture_tree" -m fixture)
git update-ref HEAD "$fixture_commit"

fail() { echo "FAIL: $*" >&2; exit 1; }
prepare() {
    git read-tree HEAD
    printf 'GOOD\n' > 'rust/domain/src/file with spaces.rs'
    rm -f "$EMUSYNC_HOOK_TEST_LOG" "$EMUSYNC_HOOK_TEST_SNAPSHOT"
}
assert_cleaned() {
    local snapshot
    snapshot=$(cat "$EMUSYNC_HOOK_TEST_SNAPSHOT")
    [[ ! -e "$snapshot" ]] || fail 'temporary snapshot survived the hook'
}

prepare
git add 'rust/domain/src/file with spaces.rs'
printf 'BAD\n' > 'rust/domain/src/file with spaces.rs'
index_before=$(git write-tree)
work_before=$(git hash-object 'rust/domain/src/file with spaces.rs')
"$BASH" .githooks/pre-commit
[[ $(cat "$EMUSYNC_HOOK_TEST_LOG") == cargo ]] || fail 'Rust changes did not run Cargo once'
[[ $(git write-tree) == "$index_before" ]] || fail 'index changed'
[[ $(git hash-object 'rust/domain/src/file with spaces.rs') == "$work_before" ]] || fail 'working copy changed'
assert_cleaned
echo 'PASS: staged good content passes despite unstaged bad content; files and index unchanged'

prepare
printf 'BAD\n' > 'rust/domain/src/file with spaces.rs'
git add 'rust/domain/src/file with spaces.rs'
printf 'GOOD\n' > 'rust/domain/src/file with spaces.rs'
index_before=$(git write-tree)
if "$BASH" .githooks/pre-commit; then fail 'bad staged content was accepted'; fi
[[ $(git write-tree) == "$index_before" ]] || fail 'failed hook changed the index'
[[ $(cat 'rust/domain/src/file with spaces.rs') == GOOD ]] || fail 'failed hook changed working content'
assert_cleaned
echo 'PASS: staged bad content blocks despite unstaged good content; failure cleans the snapshot'

prepare
printf 'Documentation\n' > README.md
git add README.md
"$BASH" .githooks/pre-commit
[[ ! -e "$EMUSYNC_HOOK_TEST_LOG" ]] || fail 'documentation ran Cargo'
echo 'PASS: documentation-only changes skip Cargo'

for config_path in Cargo.toml rust/domain/Cargo.toml rust-toolchain.toml .cargo/config.toml xtask/src/main.rs .editorconfig; do
    prepare
    mkdir -p "$(dirname "$config_path")"
    printf 'GOOD\n' > "$config_path"
    git add "$config_path"
    "$BASH" .githooks/pre-commit
    [[ -s "$EMUSYNC_HOOK_TEST_LOG" ]] || fail "$config_path did not run Cargo"
    assert_cleaned
done
echo 'PASS: manifests, toolchain and quality configuration trigger checks'

prepare
git add 'rust/domain/src/file with spaces.rs'
if EMUSYNC_HOOK_TEST_INTERRUPT=1 "$BASH" .githooks/pre-commit; then fail 'interruption was accepted'; fi
assert_cleaned
echo 'PASS: interruption cleans the snapshot'

prepare
git rm --quiet Cargo.lock
if "$BASH" .githooks/pre-commit; then fail 'commit without Cargo.lock was accepted'; fi
echo 'PASS: a staged commit without Cargo.lock is rejected'

prepare
git add 'rust/domain/src/file with spaces.rs'
git mv 'rust/domain/src/file with spaces.rs' 'rust/domain/src/renamed file.rs'
"$BASH" .githooks/pre-commit
assert_cleaned
[[ -f 'rust/domain/src/renamed file.rs' ]] || fail 'rename was lost'
echo 'PASS: renamed files work'

tree=$(git write-tree)
git -c user.name=HookTest -c user.email=hook-test@example.invalid \
    commit-tree "$tree" -m fixture > "$test_root/fixture-commit"
git update-ref HEAD "$(cat "$test_root/fixture-commit")"
git rm --quiet 'rust/domain/src/renamed file.rs'
rm -f "$EMUSYNC_HOOK_TEST_LOG"
"$BASH" .githooks/pre-commit
[[ -s "$EMUSYNC_HOOK_TEST_LOG" ]] || fail 'Rust deletion did not trigger the hook'
assert_cleaned
echo 'PASS: deleted Rust files trigger checks'

"$BASH" scripts/install-git-hooks.sh
[[ $(git config --local --get core.hooksPath) == .githooks ]] || fail 'hook not activated'
"$BASH" scripts/install-git-hooks.sh
git config --local core.hooksPath custom-hooks
if "$BASH" scripts/install-git-hooks.sh; then fail 'custom hooksPath was replaced'; fi
[[ $(git config --local --get core.hooksPath) == custom-hooks ]] || fail 'custom hooksPath changed'
git config --local --unset core.hooksPath
printf '#!/bin/sh\nexit 0\n' > .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
if "$BASH" scripts/install-git-hooks.sh; then fail 'existing active hook was bypassed'; fi
echo 'PASS: installation is idempotent and preserves existing hooks'

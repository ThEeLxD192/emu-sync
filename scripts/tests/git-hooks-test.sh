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

mkdir -p "$test_root/repo with spaces"
cd "$test_root/repo with spaces"
git init --quiet
mkdir -p .githooks scripts src config
cp "$source_root/.githooks/pre-commit" .githooks/
cp "$source_root/scripts/install-git-hooks.sh" scripts/
cat > gradlew <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "$*" == 'qualityCheck --console=plain --no-daemon' ]]
printf 'called\n' >> "$EMUSYNC_HOOK_TEST_LOG"
pwd > "$EMUSYNC_HOOK_TEST_SNAPSHOT"
if [[ -n ${EMUSYNC_HOOK_TEST_EXPECTED_PATH:-} ]]; then
    [[ $(cat "$EMUSYNC_HOOK_TEST_EXPECTED_PATH") == GOOD ]]
fi
if [[ ${EMUSYNC_HOOK_TEST_INTERRUPT:-0} == 1 ]]; then
    kill -TERM "$PPID"
    exit 0
fi
if [[ -d src ]] && find src -name '*.kt' -exec grep -l BAD {} + | grep -q .; then exit 1; fi
EOF
printf 'INITIAL\n' > 'src/Example with spaces.kt'
git add gradlew 'src/Example with spaces.kt'
fixture_tree=$(git write-tree)
fixture_commit=$(git -c user.name=HookTest -c user.email=hook-test@example.invalid commit-tree "$fixture_tree" -m fixture)
git update-ref HEAD "$fixture_commit"

fail() { echo "FAIL: $*" >&2; exit 1; }
prepare() {
    git read-tree HEAD
    git add gradlew
    printf 'GOOD\n' > 'src/Example with spaces.kt'
    rm -f "$EMUSYNC_HOOK_TEST_LOG" "$EMUSYNC_HOOK_TEST_SNAPSHOT"
}
assert_cleaned() {
    local snapshot
    snapshot=$(cat "$EMUSYNC_HOOK_TEST_SNAPSHOT")
    [[ ! -e "$snapshot" ]] || fail 'temporary snapshot survived the hook'
}

prepare
git add 'src/Example with spaces.kt'
printf 'BAD\n' > 'src/Example with spaces.kt'
index_before=$(git write-tree)
work_before=$(git hash-object 'src/Example with spaces.kt')
bash .githooks/pre-commit
[[ $(git write-tree) == "$index_before" ]] || fail 'index changed'
[[ $(git hash-object 'src/Example with spaces.kt') == "$work_before" ]] || fail 'working copy changed'
assert_cleaned
echo 'PASS: staged good content passes despite unstaged bad content; files and index unchanged'

printf 'BAD\n' > 'src/Example with spaces.kt'
git add 'src/Example with spaces.kt'
printf 'GOOD\n' > 'src/Example with spaces.kt'
index_before=$(git write-tree)
if bash .githooks/pre-commit; then fail 'bad staged content was accepted'; fi
[[ $(git write-tree) == "$index_before" ]] || fail 'failed hook changed the index'
[[ $(cat 'src/Example with spaces.kt') == GOOD ]] || fail 'failed hook changed working content'
assert_cleaned
echo 'PASS: staged bad content blocks despite unstaged good content; failure cleans the snapshot'

prepare
printf 'Documentation\n' > README.md
git add README.md
bash .githooks/pre-commit
[[ ! -e "$EMUSYNC_HOOK_TEST_LOG" ]] || fail 'documentation ran Gradle'
echo 'PASS: documentation-only changes skip Gradle'

prepare
printf 'config: {}\n' > config/detekt.yml
git add config/detekt.yml
bash .githooks/pre-commit
[[ -s "$EMUSYNC_HOOK_TEST_LOG" ]] || fail 'quality configuration did not run Gradle'
assert_cleaned
echo 'PASS: configuration changes trigger quality checks'

for config_path in \
    gradle/libs.versions.toml \
    build-logic/gradle.properties \
    build-logic/src/main/resources/META-INF/gradle-plugins/emusync.properties \
    domain/build.gradle.kts \
    data/local/build.gradle.kts \
    presentation/build.gradle.kts \
    desktopApp/build.gradle.kts; do
    prepare
    mkdir -p "$(dirname "$config_path")"
    printf 'GOOD\n' > "$config_path"
    git add "$config_path"
    printf 'BAD\n' > "$config_path"
    index_before=$(git write-tree)
    work_before=$(git hash-object "$config_path")
    EMUSYNC_HOOK_TEST_EXPECTED_PATH="$config_path" bash .githooks/pre-commit
    [[ -s "$EMUSYNC_HOOK_TEST_LOG" ]] || fail "$config_path did not run Gradle"
    [[ $(git write-tree) == "$index_before" ]] || fail "$config_path changed the index"
    [[ $(git hash-object "$config_path") == "$work_before" ]] || fail "$config_path changed working content"
    assert_cleaned
done
echo 'PASS: catalogs, build conventions and module scripts validate only staged content'

prepare
git add config/detekt.yml
if EMUSYNC_HOOK_TEST_INTERRUPT=1 bash .githooks/pre-commit; then fail 'interruption was accepted'; fi
assert_cleaned
echo 'PASS: interruption cleans the snapshot'

prepare
git add 'src/Example with spaces.kt'
git mv 'src/Example with spaces.kt' 'src/Renamed file.kt'
bash .githooks/pre-commit
assert_cleaned
[[ -f 'src/Renamed file.kt' ]] || fail 'rename was lost'
echo 'PASS: renamed files work'

# A tracked deletion still triggers when an existing tree is used as the comparison base.
tree=$(git write-tree)
git -c user.name=HookTest -c user.email=hook-test@example.invalid \
    commit-tree "$tree" -m fixture > "$test_root/fixture-commit"
git update-ref HEAD "$(cat "$test_root/fixture-commit")"
git rm --quiet 'src/Renamed file.kt'
rm -f "$EMUSYNC_HOOK_TEST_LOG"
bash .githooks/pre-commit
[[ -s "$EMUSYNC_HOOK_TEST_LOG" ]] || fail 'Kotlin deletion did not trigger the hook'
assert_cleaned
echo 'PASS: deleted Kotlin files trigger checks'

bash scripts/install-git-hooks.sh
[[ $(git config --local --get core.hooksPath) == .githooks ]] || fail 'hook not activated'
bash scripts/install-git-hooks.sh
git config --local core.hooksPath custom-hooks
if bash scripts/install-git-hooks.sh; then fail 'custom hooksPath was replaced'; fi
[[ $(git config --local --get core.hooksPath) == custom-hooks ]] || fail 'custom hooksPath changed'
git config --local --unset core.hooksPath
printf '#!/bin/sh\nexit 0\n' > .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
if bash scripts/install-git-hooks.sh; then fail 'existing active hook was bypassed'; fi
echo 'PASS: installation is idempotent and preserves existing hooks'

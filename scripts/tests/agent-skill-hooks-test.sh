#!/usr/bin/env bash
# Verifies that the shared agent hook blocks Rust edits until the required skills were read,
# through the payload shape of every supported agent.
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
hook="$repo/.agents/hooks/require_skills.py"
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
export TMPDIR=$tmp
failures=0

# Runs the hook and prints "<exit status> <stdout>" so agy's JSON decision is visible.
hook_run() {
    local agent=$1 mode=$2 payload=$3 out status=0
    out=$(printf '%s' "$payload" | python3 "$hook" --agent "$agent" "$mode" 2>/dev/null) || status=$?
    printf '%s %s' "$status" "$out"
}

expect() {
    local name=$1 expected=$2 actual=$3
    if [[ $actual != "$expected"* ]]; then
        printf 'FAIL %s: expected "%s", got "%s"\n' "$name" "$expected" "$actual"
        failures=$((failures + 1))
    fi
}

claude() { printf '{"session_id":"%s","cwd":"%s","tool_name":"%s","tool_input":%s}' "$1" "$repo" "$2" "$3"; }
skill_read() { claude "$1" Read "{\"file_path\":\"$repo/.agents/skills/$2/SKILL.md\"}"; }
code_edit='{"file_path":"rust/domain/src/lib.rs","old_string":"a","new_string":"b"}'

# Claude Code: production code needs both code skills, recorded per session.
expect "claude code edit without skills" 2 "$(hook_run claude check "$(claude s1 Edit "$code_edit")")"
hook_run claude record "$(skill_read s1 rust-development)" >/dev/null
expect "claude code edit with one skill" 2 "$(hook_run claude check "$(claude s1 Edit "$code_edit")")"
hook_run claude record "$(claude s1 Bash '{"command":"cat .agents/skills/rust-style-clippy/SKILL.md"}')" >/dev/null
expect "claude code edit with code skills" 0 "$(hook_run claude check "$(claude s1 Edit "$code_edit")")"
expect "claude other session stays blocked" 2 "$(hook_run claude check "$(claude s2 Edit "$code_edit")")"
expect "claude concurrency edit needs concurrency skill" 2 \
    "$(hook_run claude check "$(claude s1 Edit '{"file_path":"rust/domain/src/lib.rs","old_string":"a","new_string":"let m = Mutex::new(0);"}')")"
expect "claude inline test edit needs test skills" 2 \
    "$(hook_run claude check "$(claude s1 Edit '{"file_path":"rust/domain/src/lib.rs","old_string":"a","new_string":"#[test] fn t() {}"}')")"
expect "claude test file needs test skills" 2 \
    "$(hook_run claude check "$(claude s1 Write '{"file_path":"rust/application/tests/channel_tests.rs","content":"x"}')")"
hook_run claude record "$(skill_read s1 rust-valuable-tests)" >/dev/null
hook_run claude record "$(skill_read s1 rust-testing)" >/dev/null
expect "claude test file with test skills" 0 \
    "$(hook_run claude check "$(claude s1 Write '{"file_path":"rust/application/tests/channel_tests.rs","content":"x"}')")"
expect "claude non-rust file is free" 0 "$(hook_run claude check "$(claude s3 Write '{"file_path":"README.md","content":"x"}')")"
expect "claude rust outside guarded dirs is free" 0 \
    "$(hook_run claude check "$(claude s3 Write '{"file_path":"/elsewhere/lib.rs","content":"x"}')")"
hook_run claude record "$(claude s3 Write '{"file_path":"notes.md","content":"rust-development/SKILL.md rust-style-clippy/SKILL.md"}')" >/dev/null
expect "claude writing a skill path does not count as reading" 2 "$(hook_run claude check "$(claude s3 Edit "$code_edit")")"
hook_run claude record "$(claude s3 Read "{\"file_path\":\"$repo/.agents/skills/no-such-skill/SKILL.md\"}")" >/dev/null
expect "claude unknown skills are ignored" 2 "$(hook_run claude check "$(claude s3 Edit "$code_edit")")"

# Codex: apply_patch carries the edited paths inside the patch text.
codex_patch() {
    printf '{"session_id":"c1","cwd":"%s","tool_name":"apply_patch","tool_input":{"command":"*** Begin Patch\\n*** Update File: %s\\n@@\\n-a\\n+b\\n*** End Patch"}}' "$repo" "$1"
}
expect "codex patch without skills" 2 "$(hook_run codex check "$(codex_patch rust/domain/src/lib.rs)")"
expect "codex patch outside rust is free" 0 "$(hook_run codex check "$(codex_patch README.md)")"
for skill in rust-development rust-style-clippy; do
    hook_run codex record "{\"session_id\":\"c1\",\"cwd\":\"$repo\",\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"sed -n 1,200p .agents/skills/$skill/SKILL.md\"}}" >/dev/null
done
expect "codex patch with code skills" 0 "$(hook_run codex check "$(codex_patch rust/domain/src/lib.rs)")"

# Antigravity: answers with a JSON decision on stdout and always exits 0.
agy() { printf '{"conversationId":"a1","workspacePaths":["%s"],"toolCall":{"name":"%s","args":%s}}' "$repo" "$1" "$2"; }
expect "agy edit without skills" '0 {"decision": "deny"' \
    "$(hook_run agy check "$(agy replace_file_content '{"TargetFile":"'"$repo"'/rust/domain/src/lib.rs"}')")"
for skill in rust-development rust-style-clippy; do
    expect "agy record answers empty object" '0 {}' \
        "$(hook_run agy record "$(agy view_file '{"AbsolutePath":"'"$repo/.agents/skills/$skill/SKILL.md"'"}')")"
done
expect "agy edit with code skills" '0 {"decision": "allow"}' \
    "$(hook_run agy check "$(agy replace_file_content '{"TargetFile":"'"$repo"'/rust/domain/src/lib.rs"}')")"

# OpenCode: the plugin forwards V1 (`filePath`) and V2 (`path`) arguments unchanged.
opencode() { printf '{"session_id":"o1","cwd":"%s","tool":"%s","input":%s}' "$repo" "$1" "$2"; }
expect "opencode v2 edit without skills" 2 "$(hook_run opencode check "$(opencode edit '{"path":"rust/domain/src/lib.rs"}')")"
hook_run opencode record "$(opencode skill '{"id":"rust-development"}')" >/dev/null
hook_run opencode record "$(opencode read '{"filePath":"'"$repo"'/.agents/skills/rust-style-clippy/SKILL.md"}')" >/dev/null
expect "opencode v1 edit with code skills" 0 "$(hook_run opencode check "$(opencode edit '{"filePath":"rust/domain/src/lib.rs"}')")"

# A malformed payload must not block the agent.
expect "invalid input fails open" 0 "$(hook_run claude check 'not json')"
expect "invalid agy input still allows" '0 {"decision": "allow"}' "$(hook_run agy check 'not json')"

if ((failures)); then
    printf '%d agent skill hook checks failed\n' "$failures"
    exit 1
fi
echo "agent skill hook checks passed"

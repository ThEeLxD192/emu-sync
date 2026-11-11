#!/usr/bin/env bash
set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"
configured=$(git config --get core.hooksPath || true)
if [[ -n "$configured" && "$configured" != .githooks ]]; then
    echo "Refusing to replace existing core.hooksPath: $configured" >&2
    exit 1
fi
if [[ -z "$configured" ]]; then
    hooks_dir=$(git rev-parse --git-path hooks)
    for hook in "$hooks_dir"/*; do
        [[ -f "$hook" && "$hook" != *.sample && -x "$hook" ]] || continue
        echo "Refusing to bypass existing hook: $hook" >&2
        exit 1
    done
fi
chmod +x .githooks/pre-commit
git config --local core.hooksPath .githooks
echo "Installed EmuSync hooks. Commits with Rust or quality configuration changes now run cargo xtask quality."

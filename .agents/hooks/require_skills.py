#!/usr/bin/env python3
"""Block Rust edits until the agent has read the skills that AGENTS.md requires.

Shared by every coding agent through a small adapter per agent:

    require_skills.py --agent {claude,codex,agy,opencode} record   # after reading a file
    require_skills.py --agent {claude,codex,agy,opencode} check    # before editing a file

`record` remembers, per agent session, which `.agents/skills/<name>/SKILL.md`
files were read (read tools, shell commands such as `cat`/`sed`, or a native
skill tool). `check` blocks an edit of Rust code under `rust/` or `xtask/`
while a required skill is missing:

- production code: rust-development and rust-style-clippy, plus rust-concurrency
  when the edited text touches tasks, locks, atomics, channels or cancellation;
- tests (`tests/`, `tests.rs`, `*_tests.rs`, or edited text with `#[test]`,
  `#[tokio::test]`, `#[cfg(test)]`, `mod tests`): rust-valuable-tests and rust-testing.

Internal failures never block the agent: they are reported on stderr and the
call is allowed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[2]
SKILLS_DIR = ".agents/skills"
CODE_SKILLS = ("rust-development", "rust-style-clippy")
CONCURRENCY_SKILL = "rust-concurrency"
TEST_SKILLS = ("rust-valuable-tests", "rust-testing")
GUARDED_TOP_DIRS = ("rust", "xtask")

SKILL_REF = re.compile(r"([\w-]+)/SKILL\.md")
TEST_CONTENT = re.compile(r"#\[(?:tokio::)?test\b|#\[cfg\(test\)\]|\bmod tests\b")
CONCURRENCY_CONTENT = re.compile(
    r"\btokio::(?:spawn|select|sync|task)|\bspawn(?:_blocking)?\b|\bMutex\b|\bRwLock\b"
    r"|\bAtomic[A-Z]\w*|\bCancellationToken\b|\bmpsc\b|\boneshot\b|\bJoinHandle\b|\bJoinSet\b"
)
PATCH_PATH = re.compile(r"^\*\*\* (?:(?:Add|Update|Delete) File|Move to): (.+?)\s*$", re.MULTILINE)

# Tools whose arguments can show that a skill was read, per agent.
READ_TOOLS = {
    "claude": {"Read", "Bash", "Skill"},
    "codex": {"Bash", "shell", "exec_command", "local_shell", "unified_exec"},
    "agy": {"view_file", "run_command"},
    "opencode": {"read", "shell", "bash", "skill"},
}
# Tools that edit files, per agent; patch tools carry paths inside the patch text.
EDIT_TOOLS = {
    "claude": {"Edit", "MultiEdit", "Write"},
    "codex": {"apply_patch", "Edit", "Write"},
    "agy": {"write_to_file", "replace_file_content", "multi_replace_file_content"},
    "opencode": {"edit", "write", "multiedit", "patch", "apply_patch"},
}
SKILL_TOOL_ARGS = {"claude": ("skill",), "opencode": ("id", "name")}


@dataclass
class Call:
    session: str
    tool: str
    args: dict
    cwd: Path


def parse_call(agent: str, data: dict) -> Call:
    if agent == "agy":
        tool_call = data.get("toolCall") or {}
        workspaces = data.get("workspacePaths") or []
        return Call(
            session=str(data.get("conversationId") or ""),
            tool=str(tool_call.get("name") or ""),
            args=as_dict(tool_call.get("args")),
            cwd=Path(workspaces[0]) if workspaces else ROOT,
        )
    if agent == "opencode":
        return Call(
            session=str(data.get("session_id") or ""),
            tool=str(data.get("tool") or ""),
            args=as_dict(data.get("input")),
            cwd=Path(data.get("cwd") or ROOT),
        )
    # Claude Code and Codex share the same hook payload shape.
    return Call(
        session=str(data.get("session_id") or ""),
        tool=str(data.get("tool_name") or ""),
        args=as_dict(data.get("tool_input")),
        cwd=Path(data.get("cwd") or ROOT),
    )


def as_dict(value: object) -> dict:
    if isinstance(value, str):
        try:
            value = json.loads(value)
        except ValueError:
            return {"text": value}
    return value if isinstance(value, dict) else {}


def strings(value: object) -> list[str]:
    if isinstance(value, str):
        return [value]
    if isinstance(value, dict):
        return [s for v in value.values() for s in strings(v)]
    if isinstance(value, list):
        return [s for v in value for s in strings(v)]
    return []


def available_skills() -> set[str]:
    base = ROOT / SKILLS_DIR
    return {p.parent.name for p in base.glob("*/SKILL.md")} if base.is_dir() else set()


def skills_read_by(agent: str, call: Call) -> set[str]:
    if call.tool not in READ_TOOLS[agent]:
        return set()
    found = {name for text in strings(call.args) for name in SKILL_REF.findall(text)}
    if call.tool.lower() == "skill":
        for key in SKILL_TOOL_ARGS.get(agent, ()):
            value = call.args.get(key)
            if isinstance(value, str):
                found.add(value.split(":")[-1])
    return found & available_skills()


def edit_targets(call: Call) -> tuple[list[str], str]:
    """Return the edited paths and the edited text of an edit tool call."""
    args = call.args
    text = "\n".join(strings(args))
    for key in ("file_path", "filePath", "path", "TargetFile", "notebook_path"):
        value = args.get(key)
        if isinstance(value, str) and value:
            return [value], text
    return PATCH_PATH.findall(text), text


def relative_to_root(raw: str, cwd: Path) -> PurePosixPath | None:
    path = Path(raw)
    if not path.is_absolute():
        path = cwd / path
    try:
        return PurePosixPath(path.resolve().relative_to(ROOT).as_posix())
    except ValueError:
        return None


def is_test_path(rel: PurePosixPath) -> bool:
    return "tests" in rel.parts[:-1] or rel.name == "tests.rs" or rel.stem.endswith("_tests")


def required_skills(rel: PurePosixPath, text: str) -> list[str]:
    if is_test_path(rel):
        return list(TEST_SKILLS)
    skills = list(CODE_SKILLS)
    if TEST_CONTENT.search(text):
        skills += TEST_SKILLS
    if CONCURRENCY_CONTENT.search(text):
        skills.append(CONCURRENCY_SKILL)
    return skills


def missing_skills(agent: str, call: Call, done: set[str]) -> tuple[list[str], list[str]]:
    """Return the guarded files touched by an edit and the skills still unread."""
    if call.tool not in EDIT_TOOLS[agent]:
        return [], []
    paths, text = edit_targets(call)
    guarded: list[PurePosixPath] = []
    for raw in paths:
        rel = relative_to_root(raw, call.cwd)
        if rel and rel.suffix == ".rs" and rel.parts[:1] and rel.parts[0] in GUARDED_TOP_DIRS:
            guarded.append(rel)
    needed = [skill for rel in guarded for skill in required_skills(rel, text)]
    return [str(rel) for rel in guarded], [s for s in dict.fromkeys(needed) if s not in done]


def state_file(agent: str, session: str) -> Path:
    digest = hashlib.sha256(f"{ROOT}\0{agent}\0{session}".encode()).hexdigest()[:32]
    return Path(tempfile.gettempdir()) / "emusync-agent-skills" / f"{digest}.json"


def load_state(path: Path) -> set[str]:
    try:
        return set(json.loads(path.read_text(encoding="utf-8")))
    except (OSError, ValueError, TypeError):
        return set()


def save_state(path: Path, skills: set[str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(f".{os.getpid()}.tmp")
    tmp.write_text(json.dumps(sorted(skills)), encoding="utf-8")
    os.replace(tmp, path)


def deny_message(files: list[str], missing: list[str]) -> str:
    listed = "\n".join(f"  - {SKILLS_DIR}/{skill}/SKILL.md" for skill in missing)
    return (
        f"Edición de {', '.join(files)} bloqueada: antes de modificar este código lee "
        f"completas las skills que exige AGENTS.md y aplícalas:\n{listed}\n"
        "Léelas con tu herramienta de lectura (o `cat`) y después repite la edición."
    )


def respond(agent: str, mode: str, denial: str | None) -> int:
    if agent == "agy":
        if mode == "record":
            print("{}")
        elif denial:
            print(json.dumps({"decision": "deny", "reason": denial}, ensure_ascii=False))
        else:
            print(json.dumps({"decision": "allow"}))
        return 0
    if denial:
        print(denial, file=sys.stderr)
        return 2
    return 0


def run(agent: str, mode: str, data: dict) -> str | None:
    call = parse_call(agent, data)
    path = state_file(agent, call.session)
    if mode == "record":
        read = skills_read_by(agent, call)
        if read:
            save_state(path, load_state(path) | read)
        return None
    files, missing = missing_skills(agent, call, load_state(path))
    return deny_message(files, missing) if missing else None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--agent", required=True, choices=sorted(READ_TOOLS))
    parser.add_argument("mode", choices=("record", "check"))
    opts = parser.parse_args()
    try:
        data = json.load(sys.stdin)
        denial = run(opts.agent, opts.mode, data if isinstance(data, dict) else {})
    except Exception as error:  # noqa: BLE001 - a broken guard must not block the agent
        print(f"require_skills: se omite la comprobación por un error interno: {error!r}", file=sys.stderr)
        denial = None
    return respond(opts.agent, opts.mode, denial)


if __name__ == "__main__":
    sys.exit(main())

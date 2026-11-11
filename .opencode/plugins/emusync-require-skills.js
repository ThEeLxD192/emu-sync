// Blocks Rust edits until the agent has read the skills that AGENTS.md requires.
// The rules live in .agents/hooks/require_skills.py, shared with the other agents.

import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../../.agents/hooks/require_skills.py",
);
const READ_TOOLS = new Set(["read", "shell", "bash", "skill"]);
const EDIT_TOOLS = new Set(["edit", "write", "multiedit", "patch", "apply_patch"]);

function runScript(mode, payload) {
  const result = spawnSync("python3", [SCRIPT, "--agent", "opencode", mode], {
    input: JSON.stringify(payload),
    encoding: "utf8",
    timeout: 30_000,
  });
  if (result.error) {
    // A broken guard must not block the agent.
    console.error(`emusync-require-skills: ${result.error.message}`);
    return;
  }
  if (result.status === 2) {
    throw new Error(result.stderr.trim());
  }
}

function beforeTool(tool, sessionID, input, cwd) {
  const payload = { session_id: sessionID ?? "", tool, input: input ?? {}, cwd };
  if (READ_TOOLS.has(tool)) runScript("record", payload);
  if (EDIT_TOOLS.has(tool)) runScript("check", payload);
}

// OpenCode V1 hooks: `tool.execute.before(input, output)` with the tool arguments in `output.args`.
export const EmuSyncRequireSkills = async ({ directory } = {}) => ({
  "tool.execute.before": async (input, output) => {
    beforeTool(input?.tool, input?.sessionID, output?.args, directory);
  },
});

export default {
  id: "emusync.require-skills",
  server: EmuSyncRequireSkills,
  // OpenCode V2 hooks: `ctx.tool.hook("execute.before", event)` with the arguments in `event.input`.
  async setup(ctx) {
    await ctx.tool.hook("execute.before", (event) => {
      beforeTool(event.tool, event.sessionID, event.input, ctx.location?.directory);
    });
  },
};

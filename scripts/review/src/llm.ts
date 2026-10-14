// One model pass through the Claude Agent SDK: the Claude Code harness, read-only tools,
// a hard dollar cap, and a JSON-schema answer instead of prose.

import type {
  HookCallback,
  McpServerConfig,
  SDKResultMessage
} from "@anthropic-ai/claude-agent-sdk";
import { ALLOWED_TOOLS } from "../config.ts";
import { guardHook } from "./guard.ts";
import { agentEnv } from "./redact.ts";
import type { PassResult, RunPass } from "./types.ts";

const SYSTEM_APPEND = [
  "You are an automated pull-request reviewer for this repository, running in CI.",
  "You cannot edit files, post comments or ask questions. You read code and answer through the structured output tool.",
  "The repository's CLAUDE.md files are the house rules: a change that breaks one is a finding."
].join("\n");

const GRAPH_SERVER: Record<string, McpServerConfig> = {
  "code-review-graph": { type: "stdio", command: "code-review-graph", args: ["serve"] }
};

/** The guard, in the shape the SDK calls: it only ever looks at tool calls. */
function preToolUse(cwd: string): HookCallback {
  const guard = guardHook(cwd);
  return async (input) =>
    input.hook_event_name === "PreToolUse"
      ? guard({ tool_name: input.tool_name, tool_input: input.tool_input })
      : {};
}

function errorsOf(result: SDKResultMessage | null, crash: string | null): string {
  if (crash) return crash;
  return result && result.subtype !== "success" ? result.errors.join("; ").slice(0, 500) : "";
}

/**
 * Run one pass. Never throws for a model-side failure: the result says what happened,
 * so the caller can review with what finished and name what did not.
 */
export const runPass: RunPass = async <T>({
  name,
  prompt,
  schema,
  model,
  effort,
  budgetUsd,
  taskTokens,
  cwd,
  graph
}: Parameters<RunPass>[0]): Promise<PassResult<T>> => {
  const { query } = await import("@anthropic-ai/claude-agent-sdk");
  const started = Date.now();
  const tools: Record<string, number> = {};
  let result: SDKResultMessage | null = null;
  let crash: string | null = null;
  try {
    const stream = query({
      prompt,
      options: {
        model,
        effort,
        cwd,
        maxBudgetUsd: budgetUsd,
        ...(taskTokens ? { taskBudget: { total: taskTokens } } : {}),
        outputFormat: { type: "json_schema", schema },
        systemPrompt: { type: "preset", preset: "claude_code", append: SYSTEM_APPEND },
        // `project` loads the CLAUDE.md files (root, and nested ones as files are read).
        settingSources: ["project"],
        tools: ["Read", "Grep", "Glob", "Bash"],
        allowedTools: graph
          ? [...ALLOWED_TOOLS]
          : ALLOWED_TOOLS.filter((t) => !t.startsWith("mcp__")),
        permissionMode: "dontAsk",
        strictMcpConfig: true,
        mcpServers: graph ? GRAPH_SERVER : {},
        persistSession: false,
        // No GitHub token or other secret (redact.ts), and no path outside the checkout
        // (guard.ts).
        env: { ...agentEnv(process.env), CLAUDE_CODE_DISABLE_BACKGROUND_TASKS: "1" },
        hooks: { PreToolUse: [{ hooks: [preToolUse(cwd)] }] }
      }
    });
    for await (const m of stream) {
      if (m.type === "assistant") {
        for (const b of m.message.content)
          if (b.type === "tool_use") tools[b.name] = (tools[b.name] ?? 0) + 1;
      } else if (m.type === "result") {
        result = m;
      }
    }
  } catch (err) {
    crash = (err instanceof Error ? err.message : String(err)).slice(0, 500);
  }
  // The SDK validated it against `schema`; that is the contract the caller's T names.
  const output =
    result?.subtype === "success" ? ((result.structured_output as T | undefined) ?? null) : null;
  return {
    name,
    ok: output !== null,
    // The CLI can exit non-zero after it has reported (a budget stop does): the result wins.
    subtype: result?.subtype ?? (crash ? "crashed" : "no_result"),
    output,
    cost: result?.total_cost_usd ?? 0,
    ms: Date.now() - started,
    turns: result?.num_turns ?? 0,
    tools,
    error: errorsOf(result, crash)
  };
};

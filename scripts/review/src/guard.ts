// What the reviewer's tools may touch, enforced in-process before every call. The model
// reads untrusted text (the diff, files a PR adds), so a prompt injection may try to
// read secrets: /proc/<pid>/environ, a symlink the PR adds that points outside the
// checkout, `git diff --no-index` on any file. Every path must resolve inside the
// checkout, and shell commands may not chain, redirect or expand.

import { realpathSync } from "node:fs";
import { basename, dirname, isAbsolute, join, relative, resolve } from "node:path";

const SHELL_META = /[;&|`<>(){}\\\n$"]/;
// Options that make git or ast-grep read or write a file of the caller's choosing.
const GIT_FORBIDDEN = /^(--no-index|--ext-diff|--output|--exec|--textconv|--config-env|-O)/;
const BLAME_FORBIDDEN = /^(--contents|--ignore-revs-file|-S)/;
const AST_GREP_FORBIDDEN = /^(-U|--update-all|-i|--interactive)$/;
const AST_GREP_SUBCOMMANDS = new Set(["run", "scan", "outline"]);

/** The tool inputs the guard reads; anything else on the input is ignored. */
interface ToolInput {
  file_path?: unknown;
  path?: unknown;
  pattern?: unknown;
  command?: unknown;
}

export interface HookInput {
  tool_name?: string;
  tool_input?: unknown;
}

export interface HookDecision {
  hookSpecificOutput?: {
    hookEventName: "PreToolUse";
    permissionDecision: "deny";
    permissionDecisionReason: string;
  };
}

/**
 * Resolve symlinks through the deepest part of the path that exists, so a link out of
 * the checkout is caught and a name that does not exist (a revision range) still
 * compares against the same real root.
 */
function real(path: string): string {
  let head = path;
  const tail: string[] = [];
  for (;;) {
    try {
      return join(realpathSync(head), ...tail.reverse());
    } catch {
      const up = dirname(head);
      if (up === head) return path;
      tail.push(basename(head));
      head = up;
    }
  }
}

export function insideCheckout(path: unknown, cwd: string): boolean {
  if (path === undefined || path === null || path === "") return true;
  // A tool input is the model's JSON: an object here would stringify to
  // "[object Object]", a harmless-looking relative path.
  if (typeof path !== "string") return false;
  const root = real(cwd);
  const rel = relative(root, real(resolve(cwd, path)));
  return rel === "" || (!rel.startsWith("..") && !isAbsolute(rel));
}

/** Text outside single quotes: the part a shell would expand. */
const unquoted = (cmd: string): string => cmd.replace(/'[^']*'/g, "''");

/**
 * The argv a shell would build, single quotes honoured. Double quotes, backslashes and
 * every expansion are refused before this runs, so single quotes are the only quoting.
 */
export function argv(cmd: string): string[] | null {
  const out: string[] = [];
  let cur = "";
  let open = false;
  let quoted = false;
  for (const ch of cmd) {
    if (quoted) {
      if (ch === "'") quoted = false;
      else cur += ch;
    } else if (ch === "'") {
      quoted = true;
      open = true;
    } else if (/\s/.test(ch)) {
      if (open) out.push(cur);
      cur = "";
      open = false;
    } else {
      cur += ch;
      open = true;
    }
  }
  if (quoted) return null;
  if (open) out.push(cur);
  return out;
}

/** Every path an argument could name: itself, after `=`, after a short flag (`-O/x`). */
function pathsIn(arg: string): string[] {
  const candidates = [arg, arg.split("=").slice(1).join("="), arg.replace(/^-[A-Za-z]/, "")];
  return candidates.filter((c) => c.startsWith("/") || c.startsWith("~") || c.includes(".."));
}

function argProblem(
  tool: string,
  sub: string | undefined,
  arg: string,
  cwd: string
): string | null {
  for (const p of pathsIn(arg))
    if (!insideCheckout(p.replace(/^~/, "/"), cwd)) return `path outside the checkout: ${arg}`;
  const flag = arg.split("=")[0] ?? arg;
  if (tool === "git" && GIT_FORBIDDEN.test(arg))
    return `git ${flag} can read or write outside the repository`;
  if (tool === "git" && sub === "blame" && BLAME_FORBIDDEN.test(arg))
    return `git blame ${flag} reads a file of its own`;
  if (tool === "ast-grep" && AST_GREP_FORBIDDEN.test(arg))
    return "ast-grep may only search, not rewrite";
  return null;
}

function bashProblem(command: unknown, cwd: string): string | null {
  const cmd = typeof command === "string" ? command.trim() : "";
  if (SHELL_META.test(unquoted(cmd)))
    return "no pipes, redirects, chaining, substitution or double quotes";
  const args = argv(cmd);
  const [tool, sub] = args ?? [];
  if (!args || !tool) return "unbalanced quotes";
  if (tool === "ast-grep" && sub && !sub.startsWith("-") && !AST_GREP_SUBCOMMANDS.has(sub))
    return `ast-grep ${sub} is not a search`;
  for (const arg of args.slice(1)) {
    const problem = argProblem(tool, sub, arg, cwd);
    if (problem) return problem;
  }
  return null;
}

/** A glob's fixed prefix, the part before the first wildcard. */
const globRoot = (pattern: unknown): string =>
  typeof pattern === "string" ? (pattern.split(/[*?[{]/)[0] ?? "") : "";

const shown = (v: unknown): string => (typeof v === "string" ? v : JSON.stringify(v));

/** Why a tool call must be refused, or null to let it through. */
export function toolProblem(name: string | undefined, input: unknown, cwd: string): string | null {
  const i: ToolInput = typeof input === "object" && input !== null ? input : {};
  if (name === "Read" && !insideCheckout(i.file_path, cwd))
    return `read outside the checkout: ${shown(i.file_path)}`;
  if ((name === "Grep" || name === "Glob") && !insideCheckout(i.path, cwd))
    return `search outside the checkout: ${shown(i.path)}`;
  const searchRoot = typeof i.path === "string" ? i.path : ".";
  if (name === "Glob" && !insideCheckout(resolve(cwd, searchRoot, globRoot(i.pattern)), cwd))
    return `search outside the checkout: ${shown(i.pattern)}`;
  if (name === "Bash") return bashProblem(i.command, cwd);
  return null;
}

/** The PreToolUse hook the Agent SDK calls before each tool. */
export function guardHook(cwd: string): (input: HookInput) => Promise<HookDecision> {
  return async (input) => {
    const problem = toolProblem(input.tool_name, input.tool_input, cwd);
    if (!problem) return {};
    return {
      hookSpecificOutput: {
        hookEventName: "PreToolUse",
        permissionDecision: "deny",
        permissionDecisionReason: `Refused by the review sandbox: ${problem}.`
      }
    };
  };
}

// Setup commands available to flows. Fixtures may only:
//   - mutate the local working tree at known-fixed paths inside
//     `demo_project/` (and only via `reset_test_file` / `restore_demo_file:`)
//   - navigate the browser (`goto:`)
//
// Fixtures must NEVER seed, drop, or otherwise mutate any external
// system — database, warehouse, port-forward, shared service. Any new
// command that would touch a non-local endpoint requires explicit
// design review.

import { execFileSync } from "node:child_process";
import { lstatSync, realpathSync, writeFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "..");
const DEMO_PROJECT_DIR = resolve(REPO_ROOT, "demo_project");
const SCRATCH_SQL = resolve(DEMO_PROJECT_DIR, "test.sql");

export type SetupCommand = "reset_test_file" | `restore_demo_file:${string}` | `goto:${string}`;

export interface SetupContext {
  goto: (url: string) => Promise<void>;
}

export async function runSetup(commands: string[], ctx: SetupContext): Promise<void> {
  for (const cmd of commands) {
    if (cmd === "reset_test_file") {
      resetTestFile();
    } else if (cmd.startsWith("restore_demo_file:")) {
      restoreDemoFile(cmd.slice("restore_demo_file:".length));
    } else if (cmd.startsWith("goto:")) {
      await ctx.goto(cmd.slice("goto:".length));
    } else {
      throw new Error(`unknown setup command: ${cmd}`);
    }
  }
}

// Restore a `demo_project/<rel>` file to its committed-in-HEAD content
// so reruns of a flow that mutates the file (e.g. the builder editing
// insights.app.yml) start from the same canonical state. Reads from
// HEAD via `git show` so a developer's staged changes elsewhere are
// untouched.
function restoreDemoFile(rel: string): void {
  if (!rel || rel.includes("..")) {
    throw new Error(`restore_demo_file: invalid relative path ${JSON.stringify(rel)}`);
  }
  const target = resolve(DEMO_PROJECT_DIR, rel);
  const realRepoRoot = realpathSync(REPO_ROOT);
  // Same parent-walk safety check as resetTestFile: refuse symlinks and
  // any resolution that escapes the repo root.
  let probe = target;
  while (probe !== dirname(probe)) {
    try {
      const stat = lstatSync(probe);
      if (stat.isSymbolicLink()) {
        throw new Error(`restore_demo_file refuses to operate through a symbolic link: ${probe}`);
      }
      const real = realpathSync(probe);
      if (!real.startsWith(realRepoRoot)) {
        throw new Error(
          `restore_demo_file refuses to write outside the repo (resolved ${probe} → ${real})`
        );
      }
      break;
    } catch (err) {
      if ((err as NodeJS.ErrnoException).code === "ENOENT") {
        probe = dirname(probe);
        continue;
      }
      throw err;
    }
  }
  // `git show HEAD:<repo-relative path>` reads the committed blob
  // without touching the index, so a dev's staged changes elsewhere are
  // unaffected. We use a fixed cwd of REPO_ROOT to make the path
  // unambiguous regardless of where the runner was invoked from.
  const repoRel = relative(REPO_ROOT, target);
  const content = execFileSync("git", ["show", `HEAD:${repoRel}`], {
    cwd: REPO_ROOT,
    encoding: "utf-8",
    maxBuffer: 16 * 1024 * 1024
  });
  writeFileSync(target, content);
}

// Wipe `demo_project/test.sql` so the ide-save flow starts from a clean
// state on every run. Sanity-checked to refuse anything other than the
// exact in-repo path — if `demo_project` were ever symlinked outside the
// repo (or the resolution is otherwise weird), the realpath check fails
// loudly rather than writing somewhere unexpected.
function resetTestFile(): void {
  const realRepoRoot = realpathSync(REPO_ROOT);
  // Walk parents of SCRATCH_SQL until we hit something that exists.
  // Validate that the resolved path lives inside the repo.
  let probe = SCRATCH_SQL;
  while (probe !== dirname(probe)) {
    try {
      const stat = lstatSync(probe);
      if (stat.isSymbolicLink()) {
        throw new Error(`reset_test_file refuses to operate through a symbolic link: ${probe}`);
      }
      const real = realpathSync(probe);
      if (!real.startsWith(realRepoRoot)) {
        throw new Error(
          `reset_test_file refuses to write outside the repo (resolved ${probe} → ${real})`
        );
      }
      break;
    } catch (err) {
      if ((err as NodeJS.ErrnoException).code === "ENOENT") {
        probe = dirname(probe);
        continue;
      }
      throw err;
    }
  }
  writeFileSync(SCRATCH_SQL, "");
}

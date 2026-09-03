/**
 * Where a customer repo's Oxy WORKSPACE is — which is not always the repo.
 *
 * Every repo `oxyc new` scaffolds is a workspace at its root: `config.yml` sits
 * beside `semantics/`, `pipelines/` and `workflows/`. `oxyc import` brings in
 * repos that predate the template, and they are not all that shape —
 * `oxy-hq/pokehouse-oxy` is a PROJECT that CONTAINS a workspace at `oxy/`,
 * with `etl/`, `kestra/`, three apps and four workflows of its own at the root.
 * None of that is the workspace and all of it is the customer's.
 *
 * DETECTION, NEVER ASSUMPTION, and only ambiguity refuses:
 *
 *   config.yml at the repo root   → the repo IS the workspace (".")
 *   exactly one, one level down   → that subdirectory
 *   none                          → there is none; not an error
 *   two or more                   → an ERROR that names both
 *
 * The asymmetry is deliberate. Guessing between two candidates scopes a
 * session to the wrong tree and says nothing about it, which is unrecoverable
 * from the inside. Finding none is not a guess: a customer repo may hold
 * memory and notes and no workspace at all.
 *
 * THE WORKSPACE NEVER MOVES. The platform registered its subdirectory at
 * onboarding and nothing re-opens that field, so flattening a repo to match
 * the template would silently break the registration.
 */

import { existsSync, mkdirSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import * as log from "../ui/log.js";
import { CliError, ExitCode } from "../util/errors.js";

/** What makes a directory an Oxy workspace, named once. */
const WORKSPACE_MARKER = "config.yml";

/**
 * The repo's own statement about its shape.
 *
 * Its OWN file, deliberately not a field in `.oxyc-template.json`: the stamp
 * is machine-written and classified `managed`, so a sync may rewrite it — a
 * hand-written fact placed there would be destroyed by the very sync it exists
 * to inform. The stamp also answers "what was this rendered from", which is
 * about the past; this answers "what shape is this repo", which is true
 * whether oxyc rendered it or not. And an imported repo has no stamp at all,
 * while being exactly the case most likely to need an override.
 */
const REPO_CONFIG_FILE = ".oxyc.json";

/** Directories never walked when looking for a workspace. */
const SKIP_DIRS = new Set([
  ".git",
  ".github",
  "node_modules",
  "target",
  "dist",
  "out",
  ".worktrees",
  ".oxy_state",
  ".venv"
]);

/** The three outcomes of reading `.oxyc.json`. */
type Override =
  | { kind: "none" }
  | { kind: "workspace"; path: string }
  | { kind: "invalid"; why: string };

/**
 * The workspace this repo declares for itself.
 *
 * Three outcomes rather than two, because "no override" and "a broken
 * override" must not read the same: a repo that states its own shape and
 * states it WRONGLY is not a repo to guess about — the guess would silently
 * disagree with what somebody wrote down.
 */
function readOverride(repoDir: string): Override {
  const file = join(repoDir, REPO_CONFIG_FILE);
  if (!existsSync(file)) return { kind: "none" };

  let parsed: unknown;
  try {
    parsed = JSON.parse(readFileSync(file, "utf8"));
  } catch {
    return {
      kind: "invalid",
      why: `${REPO_CONFIG_FILE} is not valid JSON. Expected: {"workspace": "<subdirectory>"}`
    };
  }

  const raw =
    typeof parsed === "object" && parsed !== null
      ? (parsed as { workspace?: unknown }).workspace
      : undefined;
  // A missing key, an explicit null and an empty string all mean "nothing to
  // apply" rather than the string "null".
  if (typeof raw !== "string" || !raw.trim()) return { kind: "none" };

  const value = raw.trim();
  if (value.startsWith("/")) {
    return {
      kind: "invalid",
      why: `${REPO_CONFIG_FILE} names an absolute workspace path (${value}); it must be relative to the repo root, because the repo is cloned to a different path on every machine`
    };
  }
  if (
    value === ".." ||
    value.startsWith("../") ||
    value.endsWith("/..") ||
    value.includes("/../")
  ) {
    return {
      kind: "invalid",
      why: `${REPO_CONFIG_FILE} names a workspace outside the repo (${value}); it is a directory INSIDE the repo, or the repo itself`
    };
  }

  // `oxy/` and `oxy` are the same directory; `./` and `.` are the repo root.
  const normalized = value.replace(/\/+$/, "").replace(/^\.\//, "");
  return { kind: "workspace", path: normalized === "" ? "." : normalized };
}

/** Directories one level down that hold a `config.yml`. */
function candidates(repoDir: string): string[] {
  let entries: string[];
  try {
    entries = readdirSync(repoDir, { withFileTypes: true })
      .filter((e) => e.isDirectory() && !SKIP_DIRS.has(e.name) && !e.name.startsWith("."))
      .map((e) => e.name);
  } catch {
    return [];
  }
  return entries.filter((name) => existsSync(join(repoDir, name, WORKSPACE_MARKER))).sort();
}

/**
 * The workspace path relative to the repo root, or `undefined` when the repo
 * holds no workspace at all.
 *
 * Throws only on genuine ambiguity, and on an override that is present and
 * wrong.
 */
export function resolveWorkspace(repoDir: string): string | undefined {
  const override = readOverride(repoDir);
  if (override.kind === "invalid") {
    throw new CliError(`${repoDir} declares a workspace oxyc cannot use`, {
      code: ExitCode.REFUSED,
      detail: override.why,
      hint: "fix .oxyc.json — oxyc will not guess past a repo's own statement about its shape"
    });
  }
  if (override.kind === "workspace") {
    const abs = override.path === "." ? repoDir : join(repoDir, override.path);
    if (!existsSync(join(abs, WORKSPACE_MARKER))) {
      throw new CliError(
        `${REPO_CONFIG_FILE} names ${override.path}, which has no ${WORKSPACE_MARKER}`,
        {
          code: ExitCode.REFUSED,
          hint: "fix .oxyc.json, or add the workspace it names"
        }
      );
    }
    return override.path;
  }

  if (existsSync(join(repoDir, WORKSPACE_MARKER))) return ".";

  const found = candidates(repoDir);
  if (found.length === 1) return found[0];
  if (found.length > 1) {
    throw new CliError(`${repoDir} has more than one Oxy workspace`, {
      code: ExitCode.REFUSED,
      detail: found.map((d) => `  ${d}/${WORKSPACE_MARKER}`).join("\n"),
      hint: `say which one in ${REPO_CONFIG_FILE}: {"workspace": "<subdirectory>"}`
    });
  }
  return undefined;
}

/** The absolute workspace directory, or `undefined` when there is none. */
export function workspaceDir(repoDir: string): string | undefined {
  const rel = resolveWorkspace(repoDir);
  if (rel === undefined) return undefined;
  return rel === "." ? repoDir : join(repoDir, rel);
}

/**
 * Where a session's memory facts go — the REPO root in both layouts.
 *
 * Memory is about the CUSTOMER, not about the Oxy workspace inside their repo,
 * so it does not follow the workspace into a subdirectory. This is also the
 * one directory oxyc creates in a repo it did not scaffold.
 */
export function memoryDir(repoDir: string): string {
  return join(repoDir, "memory");
}

/**
 * The memory directory, created if absent.
 *
 * A failure warns rather than aborting: a session with nowhere to write memory
 * is still worth more than no session.
 */
export function ensureMemoryDir(repoDir: string): string {
  const dir = memoryDir(repoDir);
  try {
    mkdirSync(dir, { recursive: true });
  } catch {
    log.warn(`could not create the memory directory at ${dir}`);
  }
  return dir;
}

/**
 * `.oxyc-managed` — who owns each file in a customer repo, and which of the
 * repo's two trees each file describes.
 *
 * THE MANIFEST IS THE WHOLE AUTHORITY. Nothing in the sync or adopt engines
 * second-guesses it, and a file wrongly classified `managed` is DATA LOSS in a
 * live customer repo.
 *
 * TWO INDEPENDENT AXES, not five roles:
 *
 *   managed / scaffold / mixed        who owns the bytes
 *   scope-repo / scope-workspace      which tree the file describes
 *
 * They are independent because `package.json` is `mixed` AND repo-scoped while
 * `config.yml` is `scaffold` AND workspace-scoped — a file that could answer
 * only one question would lose the other. Each family has its OWN reader with
 * its own allowlist, so a line from one is invisible to the other and a typo
 * in either (`manged`, `scope-wrokspace`) falls to that family's default and
 * fails the completeness test rather than quietly classifying a file under a
 * word nothing recognises.
 *
 * THE TWO DEFAULTS POINT OPPOSITE WAYS, and that is the most important thing
 * in this file:
 *
 *   role  defaults to `unmatched` — the customer's, never touch. A wrong role
 *         costs data loss, so forgetting one can only ever make a sync do
 *         LESS. Most of a real repo is unmatched by construction: views,
 *         topics, pipelines, an app bundle, a lockfile, memory facts.
 *
 *   scope defaults to `repo` — always in scope, i.e. the LOUD direction. Scope
 *         cannot cost data (it never widens what a role permits), so what it
 *         trades is noise against silence. Forget a `scope-workspace` line and
 *         a file gets reported on every run until somebody writes it; forget a
 *         `scope-repo` line under the opposite default and the file can never
 *         arrive and nothing anywhere says so.
 */

import { readFileSync } from "node:fs";

import { CliError, ExitCode } from "../util/errors.js";

/** Who owns a file's bytes. */
export type Role = "managed" | "scaffold" | "mixed" | "unmatched";

/** Which of the repo's two trees a file describes. */
export type Scope = "repo" | "workspace";

interface Rule {
  pattern: string;
  value: string;
}

export interface Manifest {
  roles: Rule[];
  scopes: Rule[];
}

const ROLE_DIRECTIVES = new Set(["managed", "scaffold", "mixed"]);
const SCOPE_DIRECTIVES = new Set(["scope-repo", "scope-workspace"]);

/**
 * Parse `.oxyc-managed`.
 *
 * An unrecognised directive is IGNORED rather than rejected, which sounds
 * careless and is the safe reading: each family has its own allowlist, so a
 * typo lands in neither and the file falls to both defaults — `unmatched`
 * (never written) and `scope-repo` (always reported). The completeness test is
 * what catches it, loudly, rather than a parse error that would block every
 * command over one bad line.
 */
export function parseManifest(text: string): Manifest {
  const roles: Rule[] = [];
  const scopes: Rule[] = [];

  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const [directive, ...rest] = line.split(/\s+/);
    const pattern = rest.join(" ").trim();
    if (!directive || !pattern) continue;
    if (ROLE_DIRECTIVES.has(directive)) roles.push({ pattern, value: directive });
    else if (SCOPE_DIRECTIVES.has(directive)) {
      scopes.push({ pattern, value: directive === "scope-workspace" ? "workspace" : "repo" });
    }
  }

  return { roles, scopes };
}

/** Read the manifest that ships with the template. */
export function loadManifest(path: string): Manifest {
  try {
    return parseManifest(readFileSync(path, "utf8"));
  } catch (cause) {
    throw new CliError(`could not read the ownership manifest at ${path}`, {
      code: ExitCode.FAILURE,
      detail: (cause as Error).message,
      hint: "without it, nothing can be classified — and an unclassified file is never written"
    });
  }
}

/**
 * Glob match with shell `case` semantics, where `*` SPANS `/`.
 *
 * That is the bash tooling's behaviour and the manifest is written against it:
 * `.github/workflows/*` is meant to cover any depth beneath, and `*.gitkeep`
 * every placeholder wherever it sits. A `minimatch`-style `*` that stopped at
 * a slash would silently unmatch rules the manifest relies on — turning
 * `managed` files into `unmatched` ones, which fails safe but stops the sync
 * doing its job.
 */
export function globMatch(pattern: string, path: string): boolean {
  const escaped = pattern.replace(/[.+^${}()|[\]\\]/g, "\\$&");
  const regex = new RegExp(`^${escaped.replace(/\*/g, ".*").replace(/\?/g, ".")}$`);
  return regex.test(path);
}

/**
 * The role for a repo-relative path.
 *
 * LAST MATCHING RULE WINS, so a later, narrower line can override a broader
 * one — the same way the file reads top to bottom.
 */
export function roleFor(manifest: Manifest, path: string): Role {
  let role: Role = "unmatched";
  for (const rule of manifest.roles) {
    if (globMatch(rule.pattern, path)) role = rule.value as Role;
  }
  return role;
}

/** The scope for a repo-relative path. Defaults to `repo` — the loud direction. */
export function scopeFor(manifest: Manifest, path: string): Scope {
  let scope: Scope = "repo";
  for (const rule of manifest.scopes) {
    if (globMatch(rule.pattern, path)) scope = rule.value as Scope;
  }
  return scope;
}

/**
 * Is this template file in scope for a repo whose workspace is `workspaceRel`?
 *
 * A workspace-scoped file describes the Oxy workspace. In a repo that IS its
 * workspace (`.`) that is the root, so everything is in scope. In a repo that
 * keeps its workspace in `oxy/`, a workspace-scoped file describes nothing at
 * the root — installing it there would build a second, half-populated
 * workspace beside the real one.
 */
export function inScope(
  manifest: Manifest,
  path: string,
  workspaceRel: string | undefined
): boolean {
  if (scopeFor(manifest, path) === "repo") return true;
  return workspaceRel === "." || workspaceRel === undefined;
}

/**
 * Every template path that neither family classifies.
 *
 * Backs the completeness check: the manifest must answer BOTH questions for
 * every file the template ships, so that the two defaults are unreachable from
 * a healthy manifest and govern only one that is already failing this test.
 */
export function unclassified(
  manifest: Manifest,
  templatePaths: string[]
): {
  missingRole: string[];
  missingScope: string[];
} {
  const missingRole: string[] = [];
  const missingScope: string[] = [];
  for (const path of templatePaths) {
    if (!manifest.roles.some((r) => globMatch(r.pattern, path))) missingRole.push(path);
    if (!manifest.scopes.some((r) => globMatch(r.pattern, path))) missingScope.push(path);
  }
  return { missingRole, missingScope };
}

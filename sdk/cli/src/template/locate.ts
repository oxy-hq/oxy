/**
 * Finding the assets that ship inside this package — the customer workspace
 * template and the Claude skills.
 *
 * The package is bundled to a single `dist/main.mjs`, so the assets sit one
 * level up from the running file. That is resolved from `import.meta.url`
 * rather than `process.cwd()`, because the whole point of `npx @oxy-hq/cli` is
 * being run from a directory that has nothing to do with the package.
 *
 * `OXYC_TEMPLATE_DIR` overrides, which is what lets a developer point at a
 * working copy of the template while iterating on it — and what the tests use
 * instead of building a package.
 */

import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { CliError, ExitCode } from "../util/errors.js";

/** The package root: the directory holding `dist/`, `template/` and `skills/`. */
function packageRoot(): string {
  const here = dirname(fileURLToPath(import.meta.url));
  // Built: <pkg>/dist/main.mjs → <pkg>. Source (vitest): <pkg>/src/template → <pkg>.
  for (const candidate of [resolve(here, ".."), resolve(here, "..", "..")]) {
    if (existsSync(join(candidate, "package.json"))) return candidate;
  }
  return resolve(here, "..");
}

/**
 * The customer workspace template that `oxyc new` renders and `oxyc update`
 * compares against.
 */
export function templateDir(): string {
  const override = process.env.OXYC_TEMPLATE_DIR;
  if (override) {
    if (!existsSync(override)) {
      throw new CliError(`OXYC_TEMPLATE_DIR points at ${override}, which does not exist`, {
        code: ExitCode.USAGE
      });
    }
    return override;
  }
  const shipped = join(packageRoot(), "template");
  if (!existsSync(shipped)) {
    throw new CliError("the workspace template is missing from this installation", {
      code: ExitCode.FAILURE,
      remedy: "not on npm yet — clone the monorepo, then `pnpm --filter @oxy-hq/cli build`"
    });
  }
  return shipped;
}

/** The ownership manifest that ships with the template. */
export function manifestPath(): string {
  return join(templateDir(), ".oxyc-managed");
}

/**
 * The JSON Schemas `oxyc validate` checks against.
 *
 * Copied here from the repo root at build time by `scripts/sync-schemas.mjs`
 * and gitignored, so there is one committed source of truth — they are
 * generated from the Rust config types, and `crates/app`'s
 * `json_schemas_are_current` test is what keeps those in step.
 */
export function schemasDir(): string {
  return process.env.OXYC_SCHEMAS_DIR ?? join(packageRoot(), "json-schemas");
}

/** The Claude skills bundled in this package, linked by `oxyc skills install`. */
export function skillsDir(): string {
  return process.env.OXYC_SKILLS_DIR ?? join(packageRoot(), "skills");
}

/**
 * Where the tooling repo is, when the template came from a working copy.
 *
 * Only used for the provenance stamp — with an npm install there is no git
 * tree to describe, which the stamp records honestly as `unknown` rather than
 * inventing a commit.
 */
export function templateRepoRoot(): string {
  return process.env.OXYC_TEMPLATE_DIR
    ? resolve(process.env.OXYC_TEMPLATE_DIR, "..")
    : packageRoot();
}

/**
 * Finding the assets that ship inside this package — the customer workspace
 * template, the Claude skills, and the JSON Schemas.
 *
 * There are two shapes this tool is installed in, and they resolve differently:
 *
 *   - **A package on disk** (npm install, or a source checkout). The assets sit
 *     next to `package.json`, one level up from the running file. Resolved from
 *     `import.meta.url` rather than `process.cwd()`, because the whole point of
 *     `npx @oxy-hq/cli` is being run from a directory that has nothing to do
 *     with the package.
 *   - **A compiled single-file binary** (`bun build --compile`, what the curl
 *     installer ships). There is no package and no `dist/` — just an executable.
 *     The assets travel inside it, base64'd by `scripts/embed-assets.mjs`, and
 *     are unpacked to a cache directory the first time one is asked for.
 *
 * Disk wins whenever it is available, so an npm install and a checkout behave
 * exactly as they did before embedding existed; extraction is the fallback, not
 * the default. `OXYC_TEMPLATE_DIR` / `OXYC_SCHEMAS_DIR` / `OXYC_SKILLS_DIR`
 * override either, which is what lets a developer point at a working copy while
 * iterating — and what the tests use instead of building a package.
 */

import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

import { CliError, ExitCode } from "../util/errors.js";
import {
  ASSET_DIRS,
  embeddedAssetsDir,
  extractEmbeddedAssets,
  REINSTALL_REMEDY
} from "./embedded.js";

/**
 * The package root, when this is a real package on disk.
 *
 * Requires the asset directories to actually be present, not merely a
 * `package.json` — a compiled binary resolves `import.meta.url` to a synthetic
 * path whose parent may well contain some unrelated `package.json`, and
 * answering with that would send every lookup somewhere arbitrary.
 */
function diskPackageRoot(): string | undefined {
  const here = dirname(fileURLToPath(import.meta.url));
  // Built: <pkg>/dist/main.mjs → <pkg>. Source (vitest): <pkg>/src/template → <pkg>.
  for (const candidate of [resolve(here, ".."), resolve(here, "..", "..")]) {
    if (!existsSync(join(candidate, "package.json"))) continue;
    if (ASSET_DIRS.some((dir) => !existsSync(join(candidate, dir)))) continue;
    return candidate;
  }
  return undefined;
}

/**
 * The directory holding `template/`, `skills/` and `json-schemas/` — either the
 * installed package, or the cache the embedded copy was unpacked into.
 *
 * Memoised because extraction checks the filesystem, and a single `oxyc update`
 * asks for the template several times.
 */
let cachedRoot: string | undefined;
function packageRoot(): string {
  if (cachedRoot === undefined) cachedRoot = diskPackageRoot() ?? extractEmbeddedAssets();
  return cachedRoot;
}

/** Test seam: forget the resolved root, so an env-var change is picked up. */
export function resetPackageRootForTests(): void {
  cachedRoot = undefined;
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
      remedy: REINSTALL_REMEDY
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
 * Why a path will not still be there tomorrow, phrased for an error message —
 * or `undefined` when it looks durable.
 *
 * ONLY `skills install` needs this, and only because it SYMLINKS. Every other
 * command reads its assets and is done; a link outlives the process and is read
 * months later by something that is not this tool. Run under `npx`, the package
 * lives in npm's `_npx` cache, so `oxyc skills install` reports six skills
 * linked, in green, and every one of them dangles the moment npm reclaims that
 * cache — Claude Code then silently stops loading them, and `~/.claude/skills/`
 * still looks populated. That is the exact failure `commands/skills.ts` opens by
 * describing: links out of a tree that had to persist forever, and did not.
 *
 * WHAT IS DELIBERATELY NOT MATCHED is as important as what is. A compiled
 * binary unpacks its assets into `<cache>/oxyc/assets/<digest>/`
 * (`template/embedded.ts`) — a cache directory, and a perfectly supported place
 * to link out of, because the curl install has nowhere else to put them and the
 * path is stable for as long as that build is installed. It is EXEMPTED BY
 * NAME, first and unconditionally, rather than left to the rules below to miss:
 * `cacheDir()` is `$XDG_CACHE_HOME` or `$HOME/.cache`, both of which a sandbox
 * or an ephemeral container can perfectly well place under the temp dir — and
 * then the last rule here would refuse the one install shape that has nowhere
 * else to keep its assets, quoting a remedy (`npm install -g`) that is not even
 * how that user installed. Everything else matches package-manager EXEC caches
 * specifically, never "somewhere under a cache dir", which is why `dlx` and
 * `_npx` are exact path segments.
 *
 * The escape hatch already exists and needs no flag: `OXYC_SKILLS_DIR` pointing
 * at a durable copy is checked here like any other source, so a setup this
 * heuristic misjudges has a one-line way past it.
 */
export function ephemeralSourceReason(path: string): string | undefined {
  const full = resolve(path);

  // The compiled binary's own unpacked assets — supported, and checked before
  // anything else so no later rule can take it away.
  const assets = resolve(embeddedAssetsDir());
  if (full === assets || full.startsWith(assets + sep)) return undefined;

  const segments = full.split(sep);
  // npm's `npx` cache: ~/.npm/_npx/<hash>/node_modules/...
  if (segments.includes("_npx")) return "an `npx` cache directory";
  // `pnpm dlx` / `yarn dlx` unpack into a `dlx` directory under the store.
  if (segments.includes("dlx")) return "a `dlx` cache directory";
  // `yarn dlx` and some `bunx` paths land straight in the system temp dir.
  if (full.startsWith(resolve(tmpdir()) + sep)) return "a temporary directory";
  return undefined;
}

/**
 * Where the tooling repo is, when the template came from a working copy.
 *
 * Only used for the provenance stamp — with an npm install or a compiled binary
 * there is no git tree to describe, which the stamp records honestly as
 * `unknown` rather than inventing a commit.
 */
export function templateRepoRoot(): string {
  return process.env.OXYC_TEMPLATE_DIR
    ? resolve(process.env.OXYC_TEMPLATE_DIR, "..")
    : packageRoot();
}

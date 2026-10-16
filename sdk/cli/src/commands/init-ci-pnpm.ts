/**
 * Which pnpm and which Node the workflow `oxyc init-ci` writes builds with, and
 * what to say when the repo is not ready for that pnpm.
 *
 * The workflow installs pnpm with `pnpm/action-setup@v6`, which takes ONE
 * version: the repo's pin (package.json `packageManager`, or
 * `devEngines.packageManager`) or the step's `version:` input — and throws
 * "Multiple versions of pnpm specified" when both are present and differ. So
 * a pinned repo gets no `version:` and an unpinned one gets pnpm
 * `DEFAULT_PNPM_MAJOR`.
 *
 * `init-ci` runs in repos it did not scaffold, and some still build with
 * pnpm 10. Moving one to pnpm 11 through CI alone is not neutral: pnpm 11
 * removed `onlyBuiltDependencies` and ignores it without a word, so a repo
 * approving esbuild's postinstall that way fails its first CI install with
 * `ERR_PNPM_IGNORED_BUILDS`. That is said when the workflow is written, not
 * left for the first red run.
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { parse as parseYaml } from "yaml";

/** Installs pnpm in the build job. v6 is the first major that supports pnpm 11; v4 does not. */
export const PNPM_ACTION = "pnpm/action-setup@v6";

/**
 * What the build job installs when package.json pins no pnpm, and the Node
 * both jobs run when the repo has no `.nvmrc`: the pair the workspace template
 * pins (`template/package.json`'s `packageManager`, `template/.nvmrc`), so a
 * repo oxyc only wrote CI for builds like one it scaffolded. pnpm 11 needs
 * Node >= 22.13.
 */
export const DEFAULT_PNPM_MAJOR = "11";
export const DEFAULT_NODE_VERSION = "26";

interface PackageJson {
  packageManager?: unknown;
  devEngines?: { packageManager?: { name?: unknown; version?: unknown } } | null;
}

function readPackageJson(dir: string): PackageJson | undefined {
  try {
    return JSON.parse(readFileSync(join(dir, "package.json"), "utf8")) as PackageJson;
  } catch {
    return undefined;
  }
}

/** The pnpm version package.json pins, in either place action-setup v6 reads one. */
function pinnedPnpm(pkg: PackageJson | undefined): string | undefined {
  if (typeof pkg?.packageManager === "string" && pkg.packageManager.startsWith("pnpm@")) {
    return pkg.packageManager.slice("pnpm@".length);
  }
  const devEngine = pkg?.devEngines?.packageManager;
  if (devEngine?.name === "pnpm" && typeof devEngine.version === "string") {
    return devEngine.version;
  }
  return undefined;
}

/**
 * Whether package.json names the pnpm to install, in either place
 * pnpm/action-setup v6 reads one: `packageManager`, or
 * `devEngines.packageManager`.
 */
export function packageJsonPinsPnpm(dir: string): boolean {
  return pinnedPnpm(readPackageJson(dir)) !== undefined;
}

/** `pnpm-workspace.yaml` approves build scripts only with the key pnpm 11 removed. */
function approvesBuildsTheOldWay(dir: string): boolean {
  let settings: unknown;
  try {
    settings = parseYaml(readFileSync(join(dir, "pnpm-workspace.yaml"), "utf8"));
  } catch {
    return false;
  }
  if (typeof settings !== "object" || settings === null) return false;
  return "onlyBuiltDependencies" in settings && !("allowBuilds" in settings);
}

export interface PnpmWarning {
  warning: string;
  hint: string;
}

/**
 * What a person should hear about the pnpm the workflow will run, given the
 * repo root. Empty for a repo shaped like the template: pinned, `allowBuilds`.
 */
export function pnpmWarnings(root: string): PnpmWarning[] {
  const pin = pinnedPnpm(readPackageJson(root));
  const warnings: PnpmWarning[] = [];
  if (pin === undefined) {
    warnings.push({
      warning: `package.json pins no pnpm, so the workflow installs pnpm ${DEFAULT_PNPM_MAJOR}`,
      hint: "pin the pnpm this repo builds with — `corepack use pnpm@<version>` writes `packageManager` — and re-run with --force: the workflow then installs that one and names no version of its own"
    });
  }
  // A range or tag in a pin (`^11`, `latest`) says too little to rule pnpm 11
  // out, so only an explicit major below 11 is taken as "not pnpm 11".
  const major = Number.parseInt(pin ?? DEFAULT_PNPM_MAJOR, 10);
  if ((Number.isNaN(major) || major >= 11) && approvesBuildsTheOldWay(root)) {
    warnings.push({
      warning:
        "pnpm-workspace.yaml approves build scripts with `onlyBuiltDependencies`, which pnpm 11 ignores — the build job's install fails with ERR_PNPM_IGNORED_BUILDS",
      hint: "move each entry to an `allowBuilds` map in pnpm-workspace.yaml (`allowBuilds: { esbuild: true }`); pnpm 10.33 reads it too"
    });
  }
  return warnings;
}

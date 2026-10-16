/**
 * The scaffold's `publish.yaml`: who holds the credential, and which one.
 *
 * The workflow is YAML that no compiler reads, shipped into customer repos and
 * synced over what is there. Its security model is two properties of its
 * SHAPE — the OIDC permission lives in the job that runs no package script, and
 * a stored `OXY_TOKEN` is a fallback that wins when set — so they are pinned on
 * the parsed document rather than left to a reviewer's eye.
 */

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { parse } from "yaml";

const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const SOURCE = readFileSync(
  resolve(PACKAGE_ROOT, "template", ".github", "workflows", "publish.yaml"),
  "utf8"
);

interface Step {
  name?: string;
  uses?: string;
  run?: string;
  with?: Record<string, unknown>;
  env?: Record<string, string>;
}
interface Job {
  permissions?: Record<string, string>;
  environment?: string;
  steps: Step[];
}
interface Workflow {
  permissions: Record<string, string>;
  env: Record<string, string>;
  jobs: { build: Job; publish: Job };
}

const workflow = parse(SOURCE) as Workflow;
const { build, publish } = workflow.jobs;
const publishStep = publish.steps.find((s) => s.name === "Publish each bundle");

/** A GitHub expression, `${{ … }}`, spelled so it is not mistaken for a JS template. */
const expression = (inner: string) => ["$", "{{ ", inner, " }}"].join("");
/** A shell `${NAME:-}`, for the same reason. */
const shellVar = (name: string) => ["$", "{", name, ":-}"].join("");

/** `1.2.3` → comparable. Pre-release tags are not used on this package. */
function version(text: string): [number, number, number] {
  const [major = 0, minor = 0, patch = 0] = text
    .split(".")
    .map((part) => Number.parseInt(part, 10));
  return [major, minor, patch];
}

function newerThan(a: string, b: string): boolean {
  const [left, right] = [version(a), version(b)];
  for (let i = 0; i < 3; i++) {
    if ((left[i] ?? 0) !== (right[i] ?? 0)) return (left[i] ?? 0) > (right[i] ?? 0);
  }
  return false;
}

describe("the scaffold's publish workflow", () => {
  it("gives the OIDC permission to the publish job, bound to an environment — and to no other", () => {
    expect(workflow.permissions).toEqual({ contents: "read" });
    expect(publish.permissions).toEqual({ "id-token": "write", contents: "read" });
    expect(publish.environment).toBe(expression("needs.build.outputs.publish-env"));
    // The build job runs every dependency's install scripts: no token there.
    expect(build.permissions).toBeUndefined();
    expect(build.environment).toBeUndefined();
  });

  it("runs no package script in the job that holds the credential", () => {
    const runs = publish.steps.map((s) => s.run ?? "").join("\n");
    expect(runs).not.toMatch(/\bpnpm\b/);
    expect(runs).not.toMatch(/\bnpm run\b/);
    expect(runs).toContain("--prebuilt");
    // The CLI's own dependencies run no install scripts either.
    const install = publish.steps.find((s) => s.name === "Install the pinned oxyc CLI");
    expect(install?.env?.npm_config_ignore_scripts).toBe("true");
  });

  it("uses only first-party actions in the publish job, so the workflow resolves on day one", () => {
    // `oxy-hq/setup-oxyc` is not referenced: a scaffolded repo's CI must not
    // depend on an action it may not be able to resolve. oxyc exchanges itself.
    const actions = publish.steps.flatMap((s) => (s.uses ? [s.uses] : []));
    expect(actions.length).toBeGreaterThan(0);
    for (const action of actions) expect(action).toMatch(/^actions\//);
  });

  it("keeps a stored OXY_TOKEN as the fallback: step-scoped, and never required", () => {
    expect(publishStep?.env?.OXY_TOKEN).toBe(expression("secrets.OXY_TOKEN"));
    expect(publishStep?.env?.OXY_SERVICE_ACCOUNT).toBe(expression("vars.OXY_SERVICE_ACCOUNT"));
    // No other step, in either job, is handed the secret.
    const others = [...build.steps, ...publish.steps].filter((s) => s !== publishStep);
    for (const step of others) expect(JSON.stringify(step)).not.toContain("secrets.");

    const script = publishStep?.run ?? "";
    // A set token wins; an empty one means OIDC, not an error.
    expect(script).toContain(`if [ -n "${shellVar("OXY_TOKEN")}" ]; then`);
    expect(script).toContain(`elif [ -n "${shellVar("ACTIONS_ID_TOKEN_REQUEST_URL")}" ]; then`);
    expect(script).toContain("unset OXY_TOKEN");
    expect(script).not.toContain("no OXY_TOKEN reached this job");
  });

  /**
   * ONE SOURCE FOR THE PNPM VERSION. pnpm/action-setup v6 installs what
   * package.json's `packageManager` pins and throws "Multiple versions of pnpm
   * specified" on a `version:` input that differs from it, so the workflow
   * carries none and the template's package.json carries the pin — the same
   * one this repo builds with. v4 does not support pnpm 11.
   */
  it("installs the pnpm package.json pins, and names no version of its own", () => {
    const pnpm = build.steps.filter((s) => s.uses?.startsWith("pnpm/action-setup@"));
    expect(pnpm.map((s) => s.uses)).toEqual(["pnpm/action-setup@v6"]);
    expect(pnpm[0]?.with?.version).toBeUndefined();

    const readPackage = (path: string) =>
      JSON.parse(readFileSync(path, "utf8")) as {
        packageManager?: string;
        engines?: Record<string, string>;
      };
    const template = readPackage(resolve(PACKAGE_ROOT, "template", "package.json"));
    const repo = readPackage(resolve(PACKAGE_ROOT, "..", "..", "package.json"));
    expect(template.packageManager).toMatch(/^pnpm@11\.\d+\.\d+\+sha512\.[0-9a-f]+$/);
    expect(
      template.packageManager,
      "bump template/package.json's packageManager along with the repo root's"
    ).toBe(repo.packageManager);
    // pnpm 11 needs Node >= 22.13, and .nvmrc is what both jobs install.
    expect(template.engines).toEqual({ node: ">=22.13", pnpm: ">=11" });
    const nvmrc = readFileSync(resolve(PACKAGE_ROOT, "template", ".nvmrc"), "utf8").trim();
    expect(Number(nvmrc)).toBeGreaterThanOrEqual(22);
  });

  it("writes no secret and no untrusted value into a run line", () => {
    for (const step of [...build.steps, ...publish.steps]) {
      expect(step.run ?? "", step.name).not.toContain(["$", "{{"].join(""));
    }
  });

  /**
   * A TRIPWIRE FOR THE RELEASE THAT SHIPS TRUST POLICIES.
   *
   * The template pins the CLI its jobs install, by hand, to a version that
   * exists on npm. A pin is only worth raising to a PUBLISHED release that
   * carries `oxyc publish`'s trust-policy (OIDC self-) exchange: the newest
   * release without it is `LAST_RELEASE_WITHOUT_TRUST_POLICIES`, so the first
   * version past it is the one that ships it. Until this package's version
   * moves past that release, the exchange is unpublished and the pin cannot
   * rise. The day it does, a template still pinning that release or older
   * would scaffold repos whose default auth path (a trust policy) their own CI
   * cannot use — so bump the pin in the same change as the version: set
   * `OXYC_VERSION` in `template/.github/workflows/publish.yaml` to the version
   * being released.
   *
   * A release is not a version number: 0.6.0 was cut from a branch without the
   * exchange, after this test had assumed "after 0.5.0". If another release
   * ships without it, raise the constant to that release.
   */
  const LAST_RELEASE_WITHOUT_TRUST_POLICIES = "0.6.0";

  it("pins a CLI that knows trust policies, from the release that ships them", () => {
    const pin = workflow.env.OXYC_VERSION ?? "";
    const { version: packageVersion } = JSON.parse(
      readFileSync(resolve(PACKAGE_ROOT, "package.json"), "utf8")
    ) as { version: string };
    expect(pin).toMatch(/^\d+\.\d+\.\d+$/);
    // Never a version that has not been released yet.
    expect(newerThan(pin, packageVersion), `pin ${pin} is ahead of ${packageVersion}`).toBe(false);
    if (newerThan(packageVersion, LAST_RELEASE_WITHOUT_TRUST_POLICIES)) {
      expect(
        newerThan(pin, LAST_RELEASE_WITHOUT_TRUST_POLICIES),
        `@oxy-hq/cli is now ${packageVersion}; raise OXYC_VERSION in the template (still ${pin}) to it`
      ).toBe(true);
    }
  });
});

/**
 * The build job's `pnpm install` reads `pnpm-workspace.yaml`, and pnpm 11 fails
 * it (`ERR_PNPM_IGNORED_BUILDS`) on any build script the file does not decide.
 */
describe("the scaffold's pnpm workspace", () => {
  const settings = parse(
    readFileSync(resolve(PACKAGE_ROOT, "template", "pnpm-workspace.yaml"), "utf8")
  ) as Record<string, unknown>;

  it("approves build scripts in the key pnpm 11 reads", () => {
    // pnpm 11 removed `onlyBuiltDependencies` and ignores it without a word.
    expect(settings).not.toHaveProperty("onlyBuiltDependencies");
    expect(settings.allowBuilds).toEqual({ esbuild: true, sharp: true, "unrs-resolver": true });
  });

  it("recreates a pnpm 10 node_modules without a terminal to ask on", () => {
    // Otherwise a `pnpm install` with no terminal — an agent's, or the one
    // `oxyc publish` runs for it — aborts with
    // ERR_PNPM_ABORTED_REMOVE_MODULES_DIR_NO_TTY over a node_modules pnpm 10 wrote.
    expect(settings.confirmModulesPurge).toBe(false);
  });
});

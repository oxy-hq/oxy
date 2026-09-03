/**
 * The ownership manifest and the two engines that read it.
 *
 * These are the tests that matter most in the package: a file wrongly
 * classified `managed` is data loss in a live customer repo, and every
 * assertion below pins a rule that was learned by running the bash tooling
 * against real repos.
 */

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { planAdopt } from "./adopt.js";
import { globMatch, inScope, parseManifest, roleFor, scopeFor, unclassified } from "./manifest.js";
import { buildStamp, repoPathFor, substitute, templateSourceFor } from "./render.js";
import { computeDrift } from "./sync.js";

const MANIFEST = parseManifest(`
# comment
managed   .github/workflows/*.yaml
managed   scripts/*.sh
managed   .oxyc-template.json
scaffold  config.yml
scaffold  *.gitkeep
mixed     package.json
mixed     .gitignore

scope-repo       .github/workflows/*.yaml
scope-repo       scripts/*.sh
scope-repo       .oxyc-template.json
scope-repo       package.json
scope-repo       .gitignore
scope-repo       apps/.gitkeep
scope-workspace  config.yml
scope-workspace  semantics/*/.gitkeep
`);

/**
 * Every scratch directory this file makes, so `afterAll` can remove them.
 *
 * Without it each run leaves ~20 directories behind in `tmpdir()`. Harmless on
 * a CI runner that is thrown away; on a developer's machine it accumulates
 * across every `pnpm test`.
 */
const SCRATCH: string[] = [];

afterAll(() => {
  for (const dir of SCRATCH) rmSync(dir, { recursive: true, force: true });
  SCRATCH.length = 0;
});

function scratchRepo(files: Record<string, string>): string {
  const dir = mkdtempSync(join(tmpdir(), "oxyc-repo-"));
  SCRATCH.push(dir);
  for (const [path, content] of Object.entries(files)) {
    mkdirSync(dirname(join(dir, path)), { recursive: true });
    writeFileSync(join(dir, path), content);
  }
  return dir;
}

describe("globMatch", () => {
  /**
   * `*` SPANS `/`, matching shell `case` semantics, because the manifest is
   * written against them: `.github/workflows/*` means any depth beneath, and
   * `*.gitkeep` every placeholder wherever it sits. A minimatch-style `*` that
   * stopped at a slash would silently unmatch rules the manifest relies on.
   */
  it("lets * span a slash", () => {
    expect(globMatch(".github/workflows/*", ".github/workflows/a/b.yaml")).toBe(true);
    expect(globMatch("*.gitkeep", "semantics/views/.gitkeep")).toBe(true);
  });

  it("anchors at both ends", () => {
    expect(globMatch("config.yml", "oxy/config.yml")).toBe(false);
    expect(globMatch("config.yml", "config.yml.bak")).toBe(false);
  });

  it("escapes regex metacharacters in the pattern", () => {
    expect(globMatch("a.b", "axb")).toBe(false);
    expect(globMatch("a.b", "a.b")).toBe(true);
  });
});

describe("the two axes", () => {
  it("reads roles", () => {
    expect(roleFor(MANIFEST, ".github/workflows/publish.yaml")).toBe("managed");
    expect(roleFor(MANIFEST, "config.yml")).toBe("scaffold");
    expect(roleFor(MANIFEST, "package.json")).toBe("mixed");
  });

  /**
   * THE MOST IMPORTANT DEFAULT IN THE PACKAGE. Most of a real repo is a file
   * the template never shipped, and a rule has to be WRITTEN before anything
   * may be rewritten — so forgetting one can only ever make a sync do LESS.
   */
  it("defaults an unmatched path to `unmatched`, never to managed", () => {
    expect(roleFor(MANIFEST, "semantics/views/orders.view.yml")).toBe("unmatched");
    expect(roleFor(MANIFEST, "memory/some-fact.md")).toBe("unmatched");
  });

  /**
   * Scope defaults the OPPOSITE way — to `repo`, i.e. always in scope — because
   * scope cannot cost data, only noise. Forget a `scope-repo` line under the
   * other default and the file can never arrive and nothing says so.
   */
  it("defaults scope to `repo`, the loud direction", () => {
    expect(scopeFor(MANIFEST, "something-nobody-classified")).toBe("repo");
  });

  it("keeps the axes independent", () => {
    // `mixed` AND repo-scoped; `scaffold` AND workspace-scoped.
    expect(roleFor(MANIFEST, "package.json")).toBe("mixed");
    expect(scopeFor(MANIFEST, "package.json")).toBe("repo");
    expect(roleFor(MANIFEST, "config.yml")).toBe("scaffold");
    expect(scopeFor(MANIFEST, "config.yml")).toBe("workspace");
  });

  /**
   * A typo lands in NEITHER family's allowlist, so it falls to both defaults
   * rather than quietly classifying a file under a word nothing recognises.
   */
  it("ignores an unrecognised directive rather than guessing at it", () => {
    const typo = parseManifest("manged x.txt\nscope-wrokspace x.txt");
    expect(roleFor(typo, "x.txt")).toBe("unmatched");
    expect(scopeFor(typo, "x.txt")).toBe("repo");
    expect(unclassified(typo, ["x.txt"]).missingRole).toEqual(["x.txt"]);
  });

  it("scopes a workspace file out of a subdirectory repo", () => {
    expect(inScope(MANIFEST, "config.yml", ".")).toBe(true);
    expect(inScope(MANIFEST, "config.yml", "oxy")).toBe(false);
    // Repo-scoped files stay in scope in BOTH layouts — that was the bug: they
    // were being skipped along with the workspace's, so `package.json` could
    // never arrive in a subdirectory repo and nothing said so.
    expect(inScope(MANIFEST, "package.json", "oxy")).toBe(true);
    expect(inScope(MANIFEST, ".gitignore", "oxy")).toBe(true);
  });
});

describe("the _gitignore rename", () => {
  /**
   * npm strips a file literally named `.gitignore` from a published tarball,
   * so a template shipping one works from a checkout and arrives on npm with
   * no ignore rules — committing `node_modules/` on the first `git add -A`.
   */
  it("is an exact inverse in both directions", () => {
    expect(repoPathFor("_gitignore")).toBe(".gitignore");
    expect(templateSourceFor(".gitignore")).toBe("_gitignore");
    expect(templateSourceFor(repoPathFor("_gitignore"))).toBe("_gitignore");
    expect(repoPathFor(templateSourceFor(".gitignore"))).toBe(".gitignore");
  });

  it("does not touch an unrelated path", () => {
    expect(repoPathFor("scripts/dev.sh")).toBe("scripts/dev.sh");
    expect(templateSourceFor("docs/.gitignore.md")).toBe("docs/.gitignore.md");
  });
});

describe("substitute", () => {
  it("replaces every placeholder, everywhere", () => {
    expect(
      substitute("__SLUG__ __NAME__ __WORKSPACE__ __SLUG__", {
        slug: "acme-oxy",
        name: "Acme",
        workspace: "oxy"
      })
    ).toBe("acme-oxy Acme oxy acme-oxy");
  });

  it("leaves a file with no placeholder byte-identical", () => {
    // An empty `.gitkeep` must stay empty rather than gaining a newline, or
    // every run reports drift that is not there.
    expect(substitute("", { slug: "a", name: "b", workspace: "." })).toBe("");
  });
});

describe("the stamp", () => {
  /** `syncable` is DERIVED, so no caller can write a document whose two halves
   * disagree. */
  it("derives syncable from provenance", () => {
    expect(buildStamp({ provenance: "clean", by: "oxyc new" }).syncable).toBe(true);
    expect(buildStamp({ provenance: "imported", by: "oxyc adopt" }).syncable).toBe(false);
  });

  it("writes an unanswerable question as null rather than inventing a value", () => {
    const stamp = buildStamp({ provenance: "unknown", by: "oxyc new" });
    expect(stamp.source_commit).toBe(null);
    expect(stamp.source_branch).toBe(null);
    expect(stamp.source_dirty).toBe(null);
  });
});

describe("computeDrift", () => {
  const template = scratchRepo({
    "scripts/dev.sh": "#!/bin/sh\necho __SLUG__\n",
    "config.yml": "name: __NAME__\n",
    "package.json": '{"name":"__SLUG__"}\n',
    "semantics/views/.gitkeep": ""
  });
  const subs = { slug: "acme-oxy", name: "Acme", workspace: "." };

  it("only ever offers to write `managed` files", () => {
    const repo = scratchRepo({
      "scripts/dev.sh": "stale\n",
      "config.yml": "customised by the team\n",
      "package.json": '{"name":"acme-oxy","dependencies":{"their":"dep"}}\n'
    });
    const drift = computeDrift({
      templateDir: template,
      repoDir: repo,
      manifest: MANIFEST,
      subs,
      workspaceRel: "."
    });

    expect(drift.writable.map((e) => e.path)).toEqual(["scripts/dev.sh"]);
    // Both are reported as drifted, and neither is writable.
    const reported = drift.entries.filter((e) => e.state !== "same").map((e) => e.path);
    expect(reported).toContain("config.yml");
    expect(reported).toContain("package.json");
  });

  it("never visits a file the template does not ship", () => {
    const repo = scratchRepo({
      "scripts/dev.sh": "#!/bin/sh\necho acme-oxy\n",
      "semantics/views/orders.view.yml": "the customer's work\n"
    });
    const drift = computeDrift({
      templateDir: template,
      repoDir: repo,
      manifest: MANIFEST,
      subs,
      workspaceRel: "."
    });
    expect(drift.entries.map((e) => e.path)).not.toContain("semantics/views/orders.view.yml");
  });

  /** The comparison SUBSTITUTES, or every placeholder-bearing file reports as
   * drifted on every run — the report people stop reading. */
  it("compares against the RENDERED template, not the raw bytes", () => {
    const repo = scratchRepo({ "scripts/dev.sh": "#!/bin/sh\necho acme-oxy\n" });
    const drift = computeDrift({
      templateDir: template,
      repoDir: repo,
      manifest: MANIFEST,
      subs,
      workspaceRel: "."
    });
    expect(drift.entries.find((e) => e.path === "scripts/dev.sh")?.state).toBe("same");
  });

  it("skips workspace-scoped files in a subdirectory repo", () => {
    const repo = scratchRepo({ "oxy/config.yml": "theirs\n" });
    const drift = computeDrift({
      templateDir: template,
      repoDir: repo,
      manifest: MANIFEST,
      subs,
      workspaceRel: "oxy"
    });
    expect(drift.outOfScope).toContain("config.yml");
    expect(drift.entries.map((e) => e.path)).not.toContain("config.yml");
  });
});

describe("planAdopt", () => {
  const template = scratchRepo({
    "scripts/dev.sh": "#!/bin/sh\n",
    ".github/workflows/publish.yaml": "on: push\n",
    "package.json": '{"name":"__SLUG__"}\n',
    "config.yml": "name: __NAME__\n"
  });
  const subs = { slug: "acme-oxy", name: "Acme", workspace: "oxy" };
  const opts = (repoDir: string) => ({
    templateDir: template,
    repoDir,
    manifest: MANIFEST,
    subs,
    workspaceRel: "oxy" as string | undefined
  });

  /**
   * THE RULE THAT COST A LIVE REPO. A mixed file that is ABSENT has no
   * customer half to protect, and withholding it strands the managed files it
   * pairs with: `scripts/dev.sh` installed with no `package.json` to carry the
   * `dev` script that runs it is a tool with no handle.
   */
  it("installs an ABSENT mixed file", () => {
    const plan = planAdopt(opts(scratchRepo({ "oxy/config.yml": "theirs\n" })), false);
    expect(plan.installMixed).toContain("package.json");
  });

  it("never touches a PRESENT mixed file", () => {
    const plan = planAdopt(
      opts(scratchRepo({ "oxy/config.yml": "theirs\n", "package.json": "theirs\n" })),
      false
    );
    expect(plan.installMixed).not.toContain("package.json");
    expect(plan.collisions).not.toContain("package.json");
  });

  it("never installs a scaffold file — an imported repo has its own", () => {
    const plan = planAdopt(opts(scratchRepo({ "oxy/config.yml": "theirs\n" })), false);
    expect(plan.install).not.toContain("config.yml");
    expect(plan.installMixed).not.toContain("config.yml");
    expect(plan.outOfScope).toContain("config.yml");
  });

  it("reports a managed path that already exists as a collision", () => {
    const plan = planAdopt(
      opts(scratchRepo({ "oxy/config.yml": "x\n", ".github/workflows/publish.yaml": "theirs\n" })),
      false
    );
    expect(plan.collisions).toEqual([".github/workflows/publish.yaml"]);
  });

  /**
   * A COMPLETING run treats its own earlier output as already-here rather than
   * as a collision. Refusing there is how a repo became unreachable by both
   * commands: adopt bounced it as already-stamped, and update never restores a
   * missing mixed file.
   */
  it("finishes its own work instead of refusing it", () => {
    const plan = planAdopt(
      opts(scratchRepo({ "oxy/config.yml": "x\n", ".github/workflows/publish.yaml": "ours\n" })),
      true
    );
    expect(plan.collisions).toEqual([]);
    expect(plan.alreadyHere).toEqual([".github/workflows/publish.yaml"]);
    expect(plan.install).toContain("scripts/dev.sh");
  });
});

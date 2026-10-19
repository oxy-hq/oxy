/** The publish half of `oxy-app.json`: build steps, functions, and the strict read. */

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { CliError, ExitCode } from "../util/errors.js";
import { loadDotenv, parseDotenv } from "./dotenv.js";
import { buildSteps, declaredFunctions, functionEntry, loadPublishManifest } from "./manifest.js";

const manifest = (extra: Record<string, unknown>) => ({
  schemaVersion: 2 as const,
  slug: "s",
  ...extra
});

describe("buildSteps", () => {
  it("defaults when absent, so an identity-only manifest publishes", () => {
    expect(buildSteps(undefined)).toEqual({
      install: "pnpm install",
      command: "pnpm build",
      outDir: "out"
    });
  });

  it("takes overrides, and treats a blank value as no value", () => {
    expect(
      buildSteps(manifest({ build: { install: "npm ci", command: "  ", outDir: "dist/spa" } }))
    ).toEqual({ install: "npm ci", command: "pnpm build", outDir: "dist/spa" });
  });
});

describe("functions", () => {
  it("lists declared names and defaults each entry to functions/<name>.ts", () => {
    const m = manifest({ functions: { "top-stores": {}, rollup: { entry: "src/rollup.ts" } } });
    expect(declaredFunctions(m)).toEqual(["top-stores", "rollup"]);
    expect(functionEntry(m, "top-stores")).toBe("functions/top-stores.ts");
    expect(functionEntry(m, "rollup")).toBe("src/rollup.ts");
    expect(declaredFunctions(undefined)).toEqual([]);
  });
});

describe("loadPublishManifest", () => {
  function withDir(run: (dir: string, write: (m: unknown) => void) => void): void {
    const dir = mkdtempSync(join(tmpdir(), "oxyc-manifest-"));
    try {
      run(dir, (m) =>
        writeFileSync(join(dir, "oxy-app.json"), typeof m === "string" ? m : JSON.stringify(m))
      );
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }

  function refusalOf(fn: () => unknown): CliError {
    try {
      fn();
    } catch (cause) {
      if (cause instanceof CliError) return cause;
      throw cause;
    }
    throw new Error("expected a CliError");
  }

  it("reads the file, answers undefined for a missing one and throws for a broken one", () => {
    withDir((dir, write) => {
      expect(loadPublishManifest(dir)).toBeUndefined();
      write("{ not json");
      expect(() => loadPublishManifest(dir)).toThrow("is not a valid oxy-app.json");
      write({ functions: { "top-stores": 1 } });
      expect(() => loadPublishManifest(dir)).toThrow("`functions.top-stores` must be an object");
      write(manifest({ orgSlug: "o" }));
      expect(loadPublishManifest(dir)).toEqual({ schemaVersion: 2, slug: "s", orgSlug: "o" });
    });
  });

  /** The SDK's schema, in the SDK's words: what the server would refuse, before a build. */
  it("refuses a manifest the schema refuses, listing every problem", () => {
    withDir((dir, write) => {
      write({
        slug: "My_App",
        functions: { sync: { timeoutSeconds: 900, check: "yes" }, "../x": {} }
      });
      const error = refusalOf(() => loadPublishManifest(dir));
      expect(error.code).toBe(ExitCode.USAGE);
      expect(error.message).toMatch(/oxy-app\.json is not a valid oxy-app\.json: 5 problem\(s\)$/);
      expect(error.detail?.split("\n").map((l) => l.trim().split(":")[0])).toEqual([
        "schemaVersion",
        "slug",
        "functions.sync.timeoutSeconds",
        "functions.sync.check",
        "functions.../x"
      ]);
      expect(error.detail).toContain("`timeoutSeconds` must be an integer in [1, 300]");
      expect(error.remedy).toContain("oxyc validate");
    });
  });

  it("refuses a kind in Oxygen's reserved namespace, naming each one", () => {
    withDir((dir, write) => {
      write(
        manifest({
          functions: {
            watch: { notify: { kinds: { "oxy.anomaly": {}, late: {} } } },
            ping: { notify: { kinds: { "oxy.run_failed": { critical: true } } } }
          }
        })
      );
      const error = refusalOf(() => loadPublishManifest(dir));
      expect(error.detail?.split("\n")).toEqual([
        expect.stringMatching(/^ {2}functions\.watch\.notify\.kinds\.oxy\.anomaly: .*"oxy\."/),
        expect.stringMatching(/^ {2}functions\.ping\.notify\.kinds\.oxy\.run_failed: /)
      ]);
    });
  });

  /** Every finding at once — the server stops at the first. */
  it("lists every catalogue problem in one refusal, before any build", () => {
    withDir((dir, write) => {
      write(
        manifest({
          functions: { lists: { notify: { kinds: { late: {} } } } },
          notifications: {
            categories: { "Ops/x": {}, "a/b/c/d": {} },
            kinds: {
              late: {
                category: "finance",
                level: "urgent",
                actions: [{ id: "ack", label: "Ack", function: "ack-it" }]
              }
            }
          }
        })
      );
      const error = refusalOf(() => loadPublishManifest(dir));
      expect(error.detail?.split("\n").map((l) => l.trim().split(":")[0])).toEqual([
        "notifications.categories.Ops/x",
        "notifications.categories.a/b/c/d",
        "notifications.kinds.late.category",
        "notifications.kinds.late.level",
        "notifications.kinds.late.actions[0].function"
      ]);
    });
  });

  it("prints warnings without refusing", () => {
    withDir((dir, write) => {
      write(
        manifest({
          colour: "blue",
          functions: { f: { notify: { kinds: { late: {} } } } },
          notifications: { kinds: { late: {}, unsent: {} } }
        })
      );
      const warned: string[] = [];
      expect(loadPublishManifest(dir, (line) => warned.push(line))).toMatchObject({ slug: "s" });
      expect(warned).toEqual([
        expect.stringMatching(/^oxy-app\.json \(colour\): unknown field `colour`/),
        "oxy-app.json (notifications.kinds.unsent): no function declares it under notify.kinds, so nothing can send it yet"
      ]);
    });
  });
});

describe("dotenv", () => {
  it("parses the shapes a .env file actually has", () => {
    expect(
      parseDotenv(
        [
          "# comment",
          "OXY_ORG=acme",
          "export OXY_APP=sales",
          'QUOTED="a # not a comment"',
          "SINGLE='x=y'",
          "TRAILING=value # comment",
          'NEWLINE="a\\nb"',
          "not a line",
          "=novalue",
          ""
        ].join("\n")
      )
    ).toEqual([
      ["OXY_ORG", "acme"],
      ["OXY_APP", "sales"],
      ["QUOTED", "a # not a comment"],
      ["SINGLE", "x=y"],
      ["TRAILING", "value"],
      ["NEWLINE", "a\nb"]
    ]);
  });

  /** CI's real environment always wins; `.env.local` beats `.env`; parents are searched. */
  it("loads .env.local then .env from the nearest ancestor, never overriding", () => {
    const root = mkdtempSync(join(tmpdir(), "oxyc-dotenv-"));
    try {
      const app = join(root, "apps", "acme", "sales");
      mkdirSync(app, { recursive: true });
      writeFileSync(join(root, ".env"), "A=root\nB=root\nC=root\n");
      writeFileSync(join(app, ".env.local"), "A=local\n");
      const env: NodeJS.ProcessEnv = { C: "shell" };
      const read = loadDotenv(app, env);
      expect(read).toEqual([join(app, ".env.local"), join(root, ".env")]);
      expect(env).toEqual({ A: "local", B: "root", C: "shell" });
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});

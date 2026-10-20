/** `oxyc fn dev`'s argument grammar and seeding rules — pure, no bundling. */

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { CliError, ExitCode } from "../util/errors.js";
import {
  declaredSecrets,
  describeStagingPolicy,
  inputReader,
  mappedDestinations,
  readEnvFile,
  seedDatabases,
  seedEnv
} from "./fn-dev-seed.js";

let dir: string;
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "oxyc-fn-dev-seed-"));
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

describe("inputReader", () => {
  it("defaults to an empty JSON object", () => {
    expect(inputReader(undefined)()).toBe("{}");
  });

  it("takes a literal as the body, untouched", () => {
    expect(inputReader('{"name":"Ada"}')()).toBe('{"name":"Ada"}');
  });

  it("re-reads an @file on every call, so --watch sees an edited input", () => {
    const file = join(dir, "in.json");
    writeFileSync(file, '{"n":1}');
    const read = inputReader(`@${file}`);
    expect(read()).toBe('{"n":1}');
    writeFileSync(file, '{"n":2}');
    expect(read()).toBe('{"n":2}');
  });

  it("refuses an @file that is not there as a usage error", () => {
    const read = inputReader(`@${join(dir, "missing.json")}`);
    expect(() => read()).toThrow(CliError);
    try {
      read();
    } catch (cause) {
      expect((cause as CliError).code).toBe(ExitCode.USAGE);
    }
  });
});

describe("readEnvFile", () => {
  it("is empty when the default .env is absent", () => {
    expect(readEnvFile(dir, undefined)).toEqual({});
  });

  it("reads KEY=value lines, comments and quotes handled as publish's dotenv does", () => {
    writeFileSync(join(dir, ".env"), '# c\nSTRIPE_KEY=sk_test_1\nexport NAME="a b"\nBAD LINE\n');
    expect(readEnvFile(dir, undefined)).toEqual({ STRIPE_KEY: "sk_test_1", NAME: "a b" });
  });

  it("reads a named file, resolved against the app directory", () => {
    mkdirSync(join(dir, "secrets"));
    writeFileSync(join(dir, "secrets", "dev.env"), "K=v\n");
    expect(readEnvFile(dir, "secrets/dev.env")).toEqual({ K: "v" });
  });

  it("refuses a named file that does not exist, rather than running without secrets", () => {
    try {
      readEnvFile(dir, "nope.env");
      expect.unreachable();
    } catch (cause) {
      expect((cause as CliError).code).toBe(ExitCode.USAGE);
      expect((cause as CliError).message).toContain("nope.env");
    }
  });
});

const manifest = {
  slug: "store",
  env: { STRIPE_KEY: { required: true }, OPTIONAL: {}, TAX_KEY: { required: false } },
  nonProduction: { destinations: { clickhouse: "clickhouse_staging" } },
  functions: {
    hook: { webhook: { secretVar: "HOOK_SECRET" } },
    rollup: {
      destinations: ["clickhouse", "events", "other"],
      customerWarehouseWrites: { clickhouse: "stays in their warehouse" }
    }
  }
};

describe("seedEnv", () => {
  it("seeds ctx.env from the env file and reports required keys it lacks", () => {
    const seeded = seedEnv(manifest, "rollup", { OPTIONAL: "1" });
    expect(seeded.env).toEqual({ OPTIONAL: "1" });
    expect(seeded.unsetRequired).toEqual(["STRIPE_KEY"]);
  });

  it("is satisfied once the required key is in the file", () => {
    expect(seedEnv(manifest, "rollup", { STRIPE_KEY: "x" }).unsetRequired).toEqual([]);
  });

  it("folds a webhook's secretVar in as required, for that function only", () => {
    expect(declaredSecrets(manifest, "hook").HOOK_SECRET).toEqual({ required: true });
    expect(declaredSecrets(manifest, "rollup").HOOK_SECRET).toBeUndefined();
    expect(seedEnv(manifest, "hook", { STRIPE_KEY: "x" }).unsetRequired).toEqual(["HOOK_SECRET"]);
  });

  it("never reads the shell: a variable in process.env is not in ctx.env", () => {
    process.env.OXYC_FN_DEV_SEED_PROBE = "leak";
    try {
      expect(seedEnv(manifest, "rollup", {}).env).toEqual({});
    } finally {
      delete process.env.OXYC_FN_DEV_SEED_PROBE;
    }
  });
});

describe("seedDatabases", () => {
  it("models a destination with a customerWarehouseWrites reason as a customer warehouse", () => {
    const dbs = seedDatabases(manifest.functions.rollup);
    expect(dbs.clickhouse).toEqual({ dialect: "postgres", kind: "customer" });
  });

  it("takes every other destination to be Airhouse, and does not seed the staging mapping", () => {
    const dbs = seedDatabases(manifest.functions.rollup);
    expect(dbs.events).toEqual({ dialect: "duckdb", kind: "airhouse" });
    expect(dbs.clickhouse_staging).toBeUndefined();
    expect(Object.keys(dbs).sort()).toEqual(["clickhouse", "events", "other"]);
  });

  it("is empty for a function that names no database", () => {
    expect(seedDatabases(manifest.functions.hook)).toEqual({});
  });
});

describe("nonProduction policy", () => {
  it("reads the mapping and ignores a non-string target", () => {
    expect(mappedDestinations({ nonProduction: { destinations: { a: "b", c: 1 } } })).toEqual({
      a: "b"
    });
    expect(mappedDestinations({})).toEqual({});
  });

  it("says which destinations land somewhere in staging and which are held", () => {
    expect(describeStagingPolicy(manifest.functions.rollup, manifest)).toEqual([
      "clickhouse: lands in clickhouse_staging in staging",
      "events: held in staging (no nonProduction.destinations mapping)",
      "other: held in staging (no nonProduction.destinations mapping)"
    ]);
  });
});

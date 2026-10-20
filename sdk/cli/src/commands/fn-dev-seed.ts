/**
 * What `oxyc fn dev` seeds `createTestContext` with, read off `oxy-app.json`
 * and the env file. Pure: no bundling, no process, no clock, so the argument
 * grammar and the seeding rules are unit-testable on their own.
 */

import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { TestDatabase } from "@oxy-hq/sdk/testing";
import { parseDotenv } from "../publish/dotenv.js";
import { resolveDataInput } from "../util/data-input.js";
import { usageError } from "../util/errors.js";

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/** The env file `--env-file` defaults to, looked for in the app directory. */
export const DEFAULT_ENV_FILE = ".env";

/**
 * `--input`'s grammar is `fn call --data`'s: a literal, `@file`, or `-` for
 * stdin. A literal and stdin are read once; a `@file` is read again on every
 * run, so editing the input file under `--watch` takes effect.
 */
export function inputReader(value: string | undefined): () => string {
  if (value === undefined) return () => "{}";
  if (value.startsWith("@")) return () => resolveDataInput(value);
  const once = resolveDataInput(value);
  return () => once;
}

/**
 * The env file's `KEY=value` pairs, or `{}` when the default one is absent.
 * An env file named with `--env-file` that does not exist is a usage error:
 * the author asked for secrets and would otherwise run without them.
 */
export function readEnvFile(cwd: string, flag: string | undefined): Record<string, string> {
  const path = resolve(cwd, flag ?? DEFAULT_ENV_FILE);
  if (!existsSync(path)) {
    if (flag === undefined) return {};
    throw usageError(`--env-file ${flag} does not exist`, `looked for ${path}`);
  }
  let text: string;
  try {
    text = readFileSync(path, "utf8");
  } catch (cause) {
    throw usageError(`could not read ${path}: ${(cause as Error).message}`);
  }
  return Object.fromEntries(parseDotenv(text));
}

/** The secrets one function reads: the app's `env` keys plus its `webhook.secretVar`. */
export function declaredSecrets(
  manifest: Record<string, unknown>,
  fn: string
): Record<string, { required: boolean }> {
  const out: Record<string, { required: boolean }> = {};
  const env = isObject(manifest.env) ? manifest.env : {};
  for (const [key, decl] of Object.entries(env)) {
    out[key] = { required: isObject(decl) && decl.required === true };
  }
  const functions = isObject(manifest.functions) ? manifest.functions : {};
  const spec = functions[fn];
  const webhook = isObject(spec) && isObject(spec.webhook) ? spec.webhook : undefined;
  // The publish fold-in: a webhook's secret is declared by being named, and it
  // is always needed — a function with a `webhook` block verifies with it.
  if (webhook && typeof webhook.secretVar === "string") {
    out[webhook.secretVar] = { required: true };
  }
  return out;
}

export interface SeededEnv {
  /** `ctx.env`: the env file's pairs and nothing from the shell. */
  env: Record<string, string>;
  /** Declared `required` (or a webhook's secret) and absent from the env file. */
  unsetRequired: string[];
}

/**
 * `ctx.env` is the app's secrets, so it is seeded from the env file ONLY —
 * never from `process.env`, which would hand a handler the author's shell
 * (cloud credentials, tokens) that the host never gives it.
 */
export function seedEnv(
  manifest: Record<string, unknown>,
  fn: string,
  fileEnv: Record<string, string>
): SeededEnv {
  const declared = declaredSecrets(manifest, fn);
  const unsetRequired = Object.entries(declared)
    .filter(([key, decl]) => decl.required && fileEnv[key] === undefined)
    .map(([key]) => key)
    .sort();
  return { env: { ...fileEnv }, unsetRequired };
}

/** `nonProduction.destinations`: production database → the one staging writes instead. */
export function mappedDestinations(manifest: Record<string, unknown>): Record<string, string> {
  const block = isObject(manifest.nonProduction) ? manifest.nonProduction.destinations : undefined;
  if (!isObject(block)) return {};
  return Object.fromEntries(
    Object.entries(block).filter((entry): entry is [string, string] => typeof entry[1] === "string")
  );
}

/**
 * The databases the function may write, as `createTestContext` models them.
 *
 * A local run cannot see the workspace's `config.yml`, so the engine behind a
 * name is ASSUMED from what the manifest says: a destination with a
 * `customerWarehouseWrites` reason is a customer warehouse (read-only without
 * it, as on the host), modelled as Postgres; any other is taken to be the
 * org's Airhouse (DuckDB), which takes a write with no exception. A function
 * whose destination is really something else sees the host's refusal only once
 * published — `oxyc publish` checks that against the target.
 */
export function seedDatabases(spec: Record<string, unknown>): Record<string, TestDatabase> {
  const databases: Record<string, TestDatabase> = {};
  const reasons = isObject(spec.customerWarehouseWrites) ? spec.customerWarehouseWrites : {};
  const names = new Set<string>([
    ...(Array.isArray(spec.destinations)
      ? spec.destinations.filter((d): d is string => typeof d === "string")
      : []),
    ...Object.keys(reasons)
  ]);
  for (const name of names) {
    databases[name] =
      name in reasons
        ? { dialect: "postgres", kind: "customer" }
        : { dialect: "duckdb", kind: "airhouse" };
  }
  return databases;
}

/**
 * One line of what a non-production run does with each destination, for the
 * report: mapped (lands in the staging database) or held. Reference only —
 * the testing context keeps every write in memory and does not re-route it.
 */
export function describeStagingPolicy(
  spec: Record<string, unknown>,
  manifest: Record<string, unknown>
): string[] {
  const mapped = mappedDestinations(manifest);
  const destinations = Array.isArray(spec.destinations)
    ? spec.destinations.filter((d): d is string => typeof d === "string")
    : [];
  return destinations.map((name) =>
    mapped[name] === undefined
      ? `${name}: held in staging (no nonProduction.destinations mapping)`
      : `${name}: lands in ${mapped[name]} in staging`
  );
}

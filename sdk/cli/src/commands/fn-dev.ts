/**
 * `oxyc fn dev` — run one Oxy Function handler from the app's own source,
 * in-process, on `createTestContext` seeded from `oxy-app.json`.
 *
 * The edit-run loop `oxyc publish` + `fn call` cannot give: one changed line
 * there costs install, build, tar, upload and a remote call.
 *
 * THE HANDLER IS BUNDLED BY `bundleFunctions`, the function `oxyc publish`
 * calls — the same esbuild argv, so a handler that bundles here bundles there,
 * and the run executes the bytes publish would ship (into a temp directory
 * instead of the bundle). There is no second bundler.
 *
 * A LOCAL RUN IS NOT THE ISOLATE. It runs under Node: no timeout, memory or log
 * cap, and `Buffer`, `TextEncoder` and `process` exist where the isolate lacks
 * them. The testing context applies the manifest's capability gates and the
 * host's destination rules in the host's own words, but every warehouse,
 * semantic, storage, fetch and email call is answered by an in-memory stub —
 * nothing reaches a deployment. `--help` and the report say so; `oxyc fn call`
 * or `checks run` against a sandbox is the check that reaches the real host.
 *
 * `--read-through` sends exactly three of those calls — `ctx.query`,
 * `ctx.warehouse.query` and `ctx.semantic.query` — to a deployment, as the
 * caller; see `fn-dev-read-through.ts`. Every other call stays a stub, and the
 * report marks each call `real` or `stubbed`.
 */

import { mkdtempSync, readdirSync, rmSync, watch } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { isValidFunctionName } from "@oxy-hq/sdk/manifest";
import { createTestContext, type HostCall, type ManifestLike } from "@oxy-hq/sdk/testing";
import type { Context } from "../context/resolve.js";
import { bundleFunctions, type Runner, spawnRunner } from "../publish/functions.js";
import { declaredFunctions, functionEntry, loadPublishManifest } from "../publish/manifest.js";
import * as log from "../ui/log.js";
import { err } from "../ui/tty.js";
import { CliError, ExitCode, usageError } from "../util/errors.js";
import { createReadThrough, READ_THROUGH_OPS, type ReadThrough } from "./fn-dev-read-through.js";
import {
  describeStagingPolicy,
  inputReader,
  readEnvFile,
  seedDatabases,
  seedEnv
} from "./fn-dev-seed.js";

/** What the report and `--help` both say a local run is not. */
export const LOCAL_RUN_NOTICE =
  "LOCAL RUN, not the isolate: no timeout, memory or log limits, Node globals the isolate " +
  "lacks are present, and none of the broker's refusals that depend on a deployment. " +
  "Warehouse, semantic, storage, fetch and email calls are stubbed in memory — nothing " +
  "reaches a deployment.";

/**
 * What `--read-through` adds to the notice: the three reads go out, as the
 * caller, to `target`; everything else is still in memory. Still not the isolate.
 */
export function readThroughNotice(target: string): string {
  return (
    "LOCAL RUN, not the isolate: no timeout, memory or log limits, Node globals the isolate " +
    "lacks are present, and none of the broker's refusals that depend on a deployment. " +
    `READ-THROUGH to ${target}: ctx.query, ctx.warehouse.query and ctx.semantic.query read ` +
    "that deployment's data as you, under the server's own access checks. Every other call — " +
    "warehouse writes, transactions, oltp, airhouse, storage, fetch, email, notify — is " +
    "stubbed in memory and never leaves this machine."
  );
}

/** `--help` for `--read-through`. */
export const READ_THROUGH_HELP =
  "--read-through sends ctx.query, ctx.warehouse.query and ctx.semantic.query to the deployment " +
  "(--env <name|url> or --target), through POST /api/projects/{project}/query and /semantic-query " +
  "with your oxyc login bearer (or OXY_TOKEN); the server decides what you may read. The project is " +
  "--project / OXY_PROJECT, else looked up from oxy-app.json slug + orgSlug (or --org). Every other " +
  "call stays stubbed in memory, and the report marks each call real or stubbed.";

export interface FnDevOptions {
  /** `--input`: a literal, `@file` or `-`; the request body. Default `{}`. */
  input?: string;
  /** `--env-file`: the secrets, as `KEY=value` lines. Default `.env` when present. */
  envFile?: string;
  watch?: boolean;
  /**
   * `--read-through`: send `ctx.query`, `ctx.warehouse.query` and
   * `ctx.semantic.query` to the deployment `deps.context` targets. Needs
   * `deps.context`.
   */
  readThrough?: boolean;
}

/** What one run produced. */
export interface FnDevResult {
  function: string;
  /** The handler answered with a 2xx `Response` (or a non-`Response` value). */
  ok: boolean;
  status?: number;
  contentType?: string;
  /** The response body as text. */
  body?: string;
  /** The handler threw, or could not be loaded. */
  error?: string;
  calls: readonly HostCall[];
  /**
   * Set under `--read-through`: where the reads went. A call whose op is in
   * `ops` was `real`; every other call was `stubbed`. Absent, all are stubbed.
   */
  readThrough?: { target: string; project: string; ops: readonly string[] };
  ops: string[];
  unsetRequired: string[];
  stagingPolicy: string[];
}

/** The seams a test replaces: the esbuild runner, and where output goes. */
export interface FnDevDeps {
  run?: Runner;
  /**
   * The request body, read once per run. `runFnDev` builds it ONCE per command
   * and passes it down, so `--input -` is read from stdin once and not again
   * (at EOF, to an empty body) on each `--watch` re-run.
   */
  readInput?: () => string;
  /**
   * The command context (target, credential, `--project`/`--org`) that
   * `--read-through` sends reads with. Built by `main.ts` only when the flag is
   * set, so a run without it never resolves a target or a credential.
   */
  context?: Context;
  stdout?: (text: string) => void;
  cwd?: string;
}

type Handler = (req: unknown, ctx: unknown) => unknown;

function validateName(name: string, declared: string[]): void {
  if (!isValidFunctionName(name)) {
    throw usageError(
      `invalid function name ${JSON.stringify(name)}`,
      "function names match ^[a-z][a-z0-9-]{0,63}$"
    );
  }
  if (!declared.includes(name)) {
    throw usageError(
      `oxy-app.json declares no function "${name}"`,
      declared.length ? `declared: ${declared.join(", ")}` : "its `functions` block is empty"
    );
  }
}

/** Bundle the handler with publish's esbuild call, import it, return its default export. */
async function loadHandler(
  manifest: Parameters<typeof functionEntry>[0],
  name: string,
  cwd: string,
  run: Runner
): Promise<Handler> {
  const dir = mkdtempSync(join(tmpdir(), "oxyc-fn-dev-"));
  try {
    bundleFunctions([{ name, entry: functionEntry(manifest, name) }], cwd, dir, run);
    const url = pathToFileURL(join(dir, "functions", `${name}.js`)).href;
    const mod = (await import(url)) as { default?: unknown };
    if (typeof mod.default !== "function") {
      throw new CliError(`${functionEntry(manifest, name)} has no default-exported handler`, {
        code: ExitCode.FAILURE,
        hint: "export default async function (req, ctx) { … } — the function must return a Response"
      });
    }
    return mod.default as Handler;
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

async function readAnswer(
  value: unknown
): Promise<Pick<FnDevResult, "ok" | "status" | "contentType" | "body">> {
  if (value instanceof Response) {
    return {
      ok: value.status >= 200 && value.status < 300,
      status: value.status,
      contentType: value.headers.get("content-type") ?? undefined,
      body: await value.text()
    };
  }
  // The isolate answers with a `Response`; show what came back rather than hide it.
  return { ok: true, body: JSON.stringify(value) ?? String(value) };
}

/**
 * One run, start to finish: read the manifest and env file afresh (so
 * `--watch` sees edits to either), bundle, import, invoke on a new context.
 * A handler that throws is a result with `error`, not an exception.
 */
export async function devOnce(
  name: string,
  opts: FnDevOptions,
  deps: FnDevDeps = {}
): Promise<FnDevResult> {
  const cwd = deps.cwd ?? process.cwd();
  const manifest = loadPublishManifest(cwd);
  if (!manifest) {
    throw usageError(
      `no oxy-app.json in ${cwd}`,
      "run `oxyc fn dev` from the app directory (the one with oxy-app.json)"
    );
  }
  validateName(name, declaredFunctions(manifest));
  const spec = (manifest.functions?.[name] ?? {}) as Record<string, unknown>;
  const raw = manifest as unknown as Record<string, unknown>;
  const { env, unsetRequired } = seedEnv(raw, name, readEnvFile(cwd, opts.envFile));
  const t = createTestContext(manifest as ManifestLike, {
    function: name,
    env,
    databases: seedDatabases(spec)
  });
  // Before the handler runs: a missing login or unknown app fails the command.
  let readThrough: ReadThrough | undefined;
  if (opts.readThrough) {
    if (!deps.context) throw usageError("--read-through needs a deployment to read from");
    readThrough = await createReadThrough(deps.context, {
      slug: typeof raw.slug === "string" ? raw.slug : undefined,
      orgSlug: typeof raw.orgSlug === "string" ? raw.orgSlug : undefined
    });
    readThrough.install(t);
  }
  const base = {
    function: name,
    unsetRequired,
    stagingPolicy: describeStagingPolicy(spec, raw)
  };
  let answer: Awaited<ReturnType<typeof readAnswer>> | undefined;
  let error: string | undefined;
  try {
    const handler = await loadHandler(manifest, name, cwd, deps.run ?? spawnRunner);
    const req = {
      method: "POST",
      headers: {},
      body: (deps.readInput ?? inputReader(opts.input))()
    };
    answer = await readAnswer(await handler(req, t.ctx));
  } catch (cause) {
    // A bundling or usage failure is the command's; a handler's own throw is the answer.
    if (cause instanceof CliError) throw cause;
    const e = cause as Error;
    error = e.stack ?? e.message ?? String(cause);
  }
  return {
    ...base,
    ok: error === undefined && (answer?.ok ?? false),
    ...answer,
    error,
    readThrough: readThrough && {
      target: readThrough.target,
      project: readThrough.project,
      ops: READ_THROUGH_OPS
    },
    calls: t.calls,
    ops: t.ops()
  };
}

function show(value: unknown, max = 300): string {
  let text: string;
  try {
    text = JSON.stringify(value) ?? String(value);
  } catch {
    text = String(value);
  }
  return text.length > max ? `${text.slice(0, max)}…` : text;
}

function describeCall(call: HostCall, real: ReadonlySet<string>): string {
  const where = real.has(call.op) ? "real" : "stubbed";
  const head = `  ${call.op}  ${call.outcome}  ${where}  ${show(call.args)}`;
  return call.message ? `${head}\n    ${call.message}` : head;
}

/** The report: the response, then every host op the handler made. */
export function renderReport(result: FnDevResult): string {
  const lines: string[] = [];
  if (result.error !== undefined) {
    lines.push(`${err.red("✗")} ${result.function} threw`, result.error);
  } else {
    const status = result.status === undefined ? "" : ` ${result.status}`;
    const mark = result.ok ? err.green("✓") : err.red("✗");
    lines.push(
      `${mark} ${result.function} →${status}${result.contentType ? ` (${result.contentType})` : ""}`
    );
    if (result.body) lines.push(result.body);
  }
  lines.push("", `host ops: ${result.ops.length ? result.ops.join(", ") : "none"}`);
  if (result.readThrough) {
    lines.push(
      `read-through: ${result.readThrough.ops.join(", ")} → ${result.readThrough.target} (project ${result.readThrough.project}); every other call is stubbed`
    );
  }
  const real = new Set(result.readThrough?.ops);
  for (const call of result.calls) lines.push(describeCall(call, real));
  return `${lines.join("\n")}\n`;
}

function printNotes(result: FnDevResult): void {
  if (result.unsetRequired.length) {
    log.warn(
      `required secret(s) not in the env file: ${result.unsetRequired.join(", ")} — ctx.env.<KEY> is undefined`
    );
  }
  for (const line of result.stagingPolicy) log.info(`non-production policy: ${line}`);
}

/** Entries whose churn is the build's or the VCS's, not an edit. */
const IGNORED = /^(node_modules|\.git|dist|build|out|\.oxy[^\\/]*)$/;

/**
 * `fs.watch` on the app directory; debounced, and a change mid-run queues one
 * more run. Each top-level directory is watched on its own (recursively) so
 * `node_modules` and the build output are never walked: a recursive watch of
 * the whole app would hold a handle per directory under them.
 */
function watchApp(
  cwd: string,
  rerun: () => Promise<void>,
  onError: (e: Error) => void
): () => void {
  let timer: NodeJS.Timeout | undefined;
  let running = false;
  let dirty = false;
  const fire = async (): Promise<void> => {
    if (running) {
      dirty = true;
      return;
    }
    running = true;
    try {
      do {
        dirty = false;
        await rerun();
      } while (dirty);
    } finally {
      running = false;
    }
  };
  const changed = (): void => {
    clearTimeout(timer);
    timer = setTimeout(() => void fire(), 150);
  };
  const watchers = [
    // Top-level files (oxy-app.json, .env, package.json): not recursive.
    watch(cwd, (_event, filename) => {
      if (filename && !IGNORED.test(filename.toString())) changed();
    }),
    ...readdirSync(cwd, { withFileTypes: true })
      .filter((entry) => entry.isDirectory() && !IGNORED.test(entry.name))
      .map((entry) => watch(join(cwd, entry.name), { recursive: true }, changed))
  ];
  for (const watcher of watchers) watcher.on("error", onError);
  return () => {
    clearTimeout(timer);
    for (const watcher of watchers) watcher.close();
  };
}

/** Run, print, and report failure as `ExitCode.FAILURE`; a watch loop swallows it. */
async function runAndPrint(name: string, opts: FnDevOptions, deps: FnDevDeps): Promise<boolean> {
  const out = deps.stdout ?? ((text: string) => void process.stdout.write(text));
  const result = await devOnce(name, opts, deps);
  printNotes(result);
  out(renderReport(result));
  return result.ok;
}

export async function runFnDev(
  name: string,
  opts: FnDevOptions,
  base: FnDevDeps = {}
): Promise<void> {
  const deps: FnDevDeps = { ...base, readInput: base.readInput ?? inputReader(opts.input) };
  if (opts.readThrough && !deps.context) {
    throw usageError("--read-through needs a deployment to read from");
  }
  const notice =
    deps.context && opts.readThrough ? readThroughNotice(deps.context.target()) : LOCAL_RUN_NOTICE;
  process.stderr.write(`${err.yellow(notice)}\n`);
  if (!opts.watch) {
    if (!(await runAndPrint(name, opts, deps))) {
      throw new CliError(`${name} failed`, { code: ExitCode.FAILURE });
    }
    return;
  }
  const cwd = deps.cwd ?? process.cwd();
  const once = async (): Promise<void> => {
    try {
      await runAndPrint(name, opts, deps);
    } catch (cause) {
      // A broken edit must not end the loop: say what is wrong and wait for the next save.
      log.error(cause instanceof Error ? cause.message : String(cause));
    }
    log.info("watching for changes — Ctrl-C to stop");
  };
  await once();
  await new Promise<void>((resolve, reject) => {
    const stop = watchApp(cwd, once, (cause) => {
      stop();
      reject(new CliError(`the file watcher failed: ${cause.message}`, { code: ExitCode.FAILURE }));
    });
    process.once("SIGINT", () => {
      stop();
      resolve();
    });
  });
}

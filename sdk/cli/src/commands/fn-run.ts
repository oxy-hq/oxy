/**
 * `oxyc fn run` — queue a custom app's Oxy Function as a background run
 * (**Run now**) and wait for its verdict.
 *
 * `POST /api/customer-apps/{id}/functions/{name}/runs`, then
 * `GET …/function-runs/{run_id}` until a terminal status — the routes
 * `oxyc checks run` drives, on the staff mount a sandbox agent token also
 * reaches. Unlike `oxyc fn call` (the function's own route, as a page would
 * call it) this is the queued, system path: no caller, the org owner's
 * identity, the function's retry policy.
 *
 * `--as-schedule` runs a scheduled function once as its schedule would
 * (`internal-docs/2026-09-10-custom-app-environments-design.md` §4.5,
 * ruling 3): no input, in the named environment's build and policy, whether
 * or not that environment's schedules switch is on. The server answers `409
 * not_scheduled` for a function with no `schedule`, production included.
 * Without it, outside production only a `"check": true` function runs.
 */

import { parseJson, request } from "../api/request.js";
import { isProduction, parseAppEnv } from "../apps/environment.js";
import { ensureOk, resolveApp, staffCreds } from "../apps/resolve.js";
import { requireOwnSandbox } from "../apps/sandbox-token.js";
import { isSandboxAgentToken } from "../auth/token-kind.js";
import type { Context } from "../context/resolve.js";
import { err } from "../ui/tty.js";
import { resolveDataInput } from "../util/data-input.js";
import { CliError, ExitCode, usageError } from "../util/errors.js";

/** The `--json` report: one object on stdout. */
export interface FnRunResult {
  function: string;
  environment: string;
  runId: string;
  /** `queued` | `running` | `done` | `failed` | `cancelled` | `timed_out`. */
  status: string;
  ok: boolean;
  /** What queued it: `manual`, or `schedule_now` with `--as-schedule`. */
  trigger?: string;
  /** The function's answer, parsed when it is JSON. */
  answer?: unknown;
  error?: string;
  invocationId?: string;
}

export interface FnRunOptions {
  appEnv?: string;
  asSchedule?: boolean;
  data?: string;
  timeoutSeconds: number;
  pollMs?: number;
}

interface RunDetail {
  status: string;
  answer?: string | null;
  error?: string | null;
  trigger?: string | null;
  invocation_id?: string | null;
}

const TERMINAL = new Set(["done", "failed", "cancelled"]);
const SURFACE = "/api/customer-apps";

/** Usage errors, before any request. */
function validate(opts: FnRunOptions): string | undefined {
  if (!Number.isFinite(opts.timeoutSeconds) || opts.timeoutSeconds <= 0) {
    throw usageError(
      "--timeout must be a positive number of seconds",
      "e.g. --timeout 60 (the default is 300)"
    );
  }
  if (opts.asSchedule && opts.data !== undefined) {
    throw usageError(
      "--as-schedule takes no --data",
      "a scheduled run has no input — the server refuses a body with 400 input_refused"
    );
  }
  return opts.appEnv === undefined ? undefined : parseAppEnv(opts.appEnv);
}

function runQuery(appEnv: string | undefined, asSchedule: boolean): string {
  const query = new URLSearchParams();
  if (appEnv !== undefined) query.set("environment", appEnv);
  if (asSchedule) query.set("as", "schedule");
  const text = query.toString();
  return text ? `?${text}` : "";
}

/**
 * Queue the run and poll it to a terminal status or the client-side
 * timeout. No printing and no throw for a run that failed (`ok: false`) —
 * that decision belongs to the caller: `runFnRun` prints and exits, the
 * `oxyc mcp` tool returns the result as-is.
 */
export async function fnRun(
  ctx: Context,
  app: string,
  fn: string,
  opts: FnRunOptions
): Promise<FnRunResult> {
  const appEnv = validate(opts);
  const creds = staffCreds(ctx);
  if (isSandboxAgentToken(creds.bearer)) requireOwnSandbox("oxyc fn run", appEnv);
  const { appId } = await resolveApp(creds, app);
  const asSchedule = Boolean(opts.asSchedule);
  const queued = await request({
    target: creds.target,
    path: `${SURFACE}/${appId}/functions/${encodeURIComponent(fn)}/runs${runQuery(appEnv, asSchedule)}`,
    method: "POST",
    body: asSchedule || opts.data === undefined ? undefined : resolveDataInput(opts.data),
    bearer: creds.bearer,
    headers: creds.headers
  });
  ensureOk(queued, creds);
  const payload = parseJson(queued.body) as { run_id?: string; environment?: string } | undefined;
  if (!payload?.run_id) {
    throw new CliError(`POST …/functions/${fn}/runs did not return a run_id`, {
      code: ExitCode.UNAVAILABLE
    });
  }
  const detailPath = `${SURFACE}/${appId}/function-runs/${payload.run_id}${runQuery(appEnv, false)}`;
  const detail = await pollUntilTerminal(creds, detailPath, opts);
  return {
    function: fn,
    environment: payload.environment ?? appEnv ?? "production",
    runId: payload.run_id,
    status: detail.status,
    ok: detail.status === "done",
    trigger: detail.trigger ?? undefined,
    answer: detail.answer ? (parseJson(detail.answer) ?? detail.answer) : undefined,
    error: detail.status === "done" ? undefined : failure(detail, opts.timeoutSeconds),
    invocationId: detail.invocation_id ?? undefined
  };
}

async function pollUntilTerminal(
  creds: ReturnType<typeof staffCreds>,
  path: string,
  opts: FnRunOptions
): Promise<RunDetail> {
  const deadline = Date.now() + opts.timeoutSeconds * 1000;
  for (;;) {
    const response = await request({
      target: creds.target,
      path,
      method: "GET",
      bearer: creds.bearer,
      headers: creds.headers
    });
    ensureOk(response, creds);
    const detail = parseJson(response.body) as RunDetail;
    if (TERMINAL.has(detail.status)) return detail;
    if (Date.now() >= deadline) return { ...detail, status: "timed_out" };
    await new Promise((resolve) => setTimeout(resolve, opts.pollMs ?? 2000));
  }
}

function failure(detail: RunDetail, timeoutSeconds: number): string {
  if (detail.error) return detail.error;
  if (detail.status === "timed_out") return `timed out after ${timeoutSeconds}s`;
  return `status ${detail.status}`;
}

export async function runFnRun(
  ctx: Context,
  app: string,
  fn: string,
  opts: FnRunOptions & { json: boolean }
): Promise<void> {
  const result = await fnRun(ctx, app, fn, opts);
  if (opts.json) {
    process.stdout.write(`${JSON.stringify(result)}\n`);
  } else if (!process.env.OXYC_QUIET) {
    const where = isProduction(result.environment) ? "" : ` in ${result.environment}`;
    if (result.ok) process.stderr.write(`${err.green("✓")} ${fn}${where} → done\n`);
    else process.stderr.write(`${err.red("✗")} ${fn}${where} failed: ${result.error}\n`);
  }
  if (!result.ok) {
    throw new CliError(result.error ?? `${fn} failed`, { code: ExitCode.FAILURE });
  }
}

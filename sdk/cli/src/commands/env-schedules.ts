/**
 * `oxyc env schedules <app> <name> [--on|--off]` — a non-production
 * environment's schedules switch
 * (`internal-docs/2026-09-10-custom-app-environments-design.md` §4.5,
 * ruling 1).
 *
 * Staging and every `dev-<handle>` sandbox fire their functions' `schedule`s
 * only once staff turn the switch on — off by default, because a schedule
 * there reads production data on a timer and calls third parties with
 * staging's credentials. Production has no switch: its schedules always run.
 *
 * `GET` / `PUT /api/customer-apps/{id}/environments/{name}/schedules`
 * (`crates/app/src/server/api/custom_apps_env_schedules/`). With neither flag
 * this reads the switch and the environment's schedule rows back. Staff only:
 * a publish token is refused by `staffCreds`, and a sandbox agent token here,
 * before any request — the server refuses it too.
 */

import { parseJson, request } from "../api/request.js";
import { parseAppEnv } from "../apps/environment.js";
import { ensureOk, resolveApp, staffCreds } from "../apps/resolve.js";
import { isSandboxAgentToken } from "../auth/token-kind.js";
import type { Context } from "../context/resolve.js";
import { usageError } from "../util/errors.js";

/** One schedule row of the environment, as the server answers it. */
export interface EnvironmentScheduleRow {
  id: string;
  function: string;
  cron_expr: string;
  timezone: string;
  next_run_at: string;
  last_fired_at: string | null;
  last_run_id: string | null;
  last_error: string | null;
}

/** The server's read-back — snake_case, printed verbatim with `--json`. */
export interface EnvironmentSchedules {
  environment: string;
  enabled: boolean;
  schedules: EnvironmentScheduleRow[];
  /** What the reconcile behind a flip could not do; `[]` on a read. */
  warnings: string[];
}

/** Read the switch, or set it when `enabled` is given. No printing. */
export async function envSchedules(
  ctx: Context,
  app: string,
  name: string,
  enabled?: boolean
): Promise<EnvironmentSchedules> {
  const environment = parseAppEnv(name);
  if (environment === "production") {
    throw usageError(
      "production has no schedules switch",
      "its functions' schedules always run — name staging or a dev-<handle> sandbox"
    );
  }
  const creds = staffCreds(ctx);
  if (isSandboxAgentToken(creds.bearer)) {
    throw usageError(
      "a sandbox agent token cannot read or flip an environment's schedules",
      "staff turn schedules on: ask your operator to run `oxyc env schedules <app> <name> --on`, or run the function once with `oxyc fn run … --as-schedule`"
    );
  }
  const { appId } = await resolveApp(creds, app);
  const response = await request({
    target: creds.target,
    path: `/api/customer-apps/${appId}/environments/${encodeURIComponent(environment)}/schedules`,
    method: enabled === undefined ? "GET" : "PUT",
    body: enabled === undefined ? undefined : JSON.stringify({ enabled }),
    bearer: creds.bearer,
    headers: creds.headers
  });
  ensureOk(response, creds);
  return parseJson(response.body) as EnvironmentSchedules;
}

/** One line per schedule row, for the human output. */
export function describeSchedules(shown: EnvironmentSchedules): string[] {
  const state = shown.enabled ? "on" : "off";
  const lines = [`${shown.environment} — schedules ${state}`];
  for (const row of shown.schedules) {
    const last = row.last_error ? `; last error: ${row.last_error}` : "";
    lines.push(
      `  ${row.function}  ${row.cron_expr} (${row.timezone}), next ${row.next_run_at}${last}`
    );
  }
  if (shown.enabled && shown.schedules.length === 0) {
    lines.push("  no function in the build this environment serves declares a schedule");
  }
  for (const warning of shown.warnings) lines.push(`  warning: ${warning}`);
  return lines;
}

export async function runEnvSchedules(
  ctx: Context,
  app: string,
  name: string,
  opts: { on?: boolean; off?: boolean; json: boolean }
): Promise<void> {
  if (opts.on && opts.off) {
    throw usageError("--on and --off together", "pass one, or neither to read the switch");
  }
  const enabled = opts.on ? true : opts.off ? false : undefined;
  const shown = await envSchedules(ctx, app, name, enabled);
  if (opts.json) {
    process.stdout.write(`${JSON.stringify(shown)}\n`);
    return;
  }
  process.stdout.write(`${describeSchedules(shown).join("\n")}\n`);
}

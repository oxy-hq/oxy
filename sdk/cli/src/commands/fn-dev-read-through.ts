/**
 * `oxyc fn dev --read-through` — the transport that sends a handler's three
 * read ops to a deployment.
 *
 * WHAT GOES OUT, AND ONLY THAT. `READ_THROUGH_OPS` is the whole allow-list:
 * `query`, `warehouse.query` and `semantic.query` (what `ctx.query`,
 * `ctx.warehouse.query` and `ctx.semantic.query` pack their arguments into).
 * Each is installed as an override of that one host op on the testing context;
 * every other op — `warehouse.insert`/`exec`/`upsert`, `tx.*`, `oltp.*`,
 * `airhouse.*`, `query_stream`, `storage.*`, `fetch`, `email.send`, `notify`,
 * `airway.run`, `platform.facts` — keeps the context's in-memory answer and
 * never reaches the network. This module has no code path to any other
 * endpoint: adding an op to the list is the only way to widen it.
 *
 * THE ROUTES ARE THE ONES A PUBLISHED APP'S BROWSER SDK CALLS
 * (`query-cache.ts` and `useSemanticQuery` in `@oxy-hq/sdk`):
 *
 *   ctx.query / ctx.warehouse.query -> POST /api/projects/{project}/query
 *   ctx.semantic.query              -> POST /api/projects/{project}/semantic-query
 *
 * WHAT MAY BE READ IS THE SERVER'S DECISION. Those routes run
 * `check_custom_app_gates` (authentication, project access, the SELECT-only
 * gate, the row cap) for whoever the bearer is; nothing here re-implements or
 * relaxes any of it. The bearer is the one `oxyc api` sends (`ctx.bearer()`),
 * and a sandbox agent token is refused first for the same reason `oxyc api`
 * refuses it.
 *
 * The routes answer `{ columns, rows: unknown[][], truncated }`; the host ops
 * answer `{ rows: object[], truncated }`, so the rows are turned back into
 * objects here. The row cap and byte cap are the route's, not a function's.
 */

import { errorForResponse, parseJson, request } from "../api/request.js";
import { refuseSandboxToken } from "../apps/sandbox-token.js";
import type { Context } from "../context/resolve.js";
import { fetchProject } from "../publish/server.js";
import { usageError } from "../util/errors.js";

/** The host ops sent to the deployment. Everything else stays in memory. */
export const READ_THROUGH_OPS = ["query", "warehouse.query", "semantic.query"] as const;
export type ReadThroughOp = (typeof READ_THROUGH_OPS)[number];

/** What a host-op override takes and answers (the testing context's `HostOpImpl`). */
type HostOpImpl = (args: Record<string, unknown>) => Promise<unknown>;

/** The hosting side of the testing context that the transport installs onto. */
export interface OverridableContext {
  override(op: string, impl: HostOpImpl): void;
}

export interface ReadThrough {
  readonly target: string;
  readonly project: string;
  /** Whether a recorded host call went to the deployment. */
  handles(op: string): boolean;
  /** Replace exactly the allow-listed ops on `t`; every other op is untouched. */
  install(t: OverridableContext): void;
}

/** The slice of `oxy-app.json` that names the app. */
export interface ReadThroughApp {
  slug?: string;
  orgSlug?: string;
}

function envValue(name: string): string | undefined {
  return process.env[name]?.trim() || undefined;
}

/**
 * The project the app's reads run against, the way `oxyc publish` finds it:
 * `--project` / `OXY_PROJECT`, else the target's public build-config lookup by
 * `(org, slug)`.
 */
async function resolveProject(ctx: Context, target: string, app: ReadThroughApp): Promise<string> {
  const pinned = ctx.flags.project?.trim() || envValue("OXY_PROJECT");
  if (pinned) return pinned;
  const org = ctx.flags.org?.trim() || envValue("OXY_ORG") || app.orgSlug || ctx.env().orgSlug;
  if (!org || !app.slug) {
    throw usageError(
      "--read-through needs to know which project to read",
      "pass --project <uuid> (or OXY_PROJECT), or set oxy-app.json `slug` and `orgSlug` (or --org <slug>) so the project can be looked up on the target"
    );
  }
  return fetchProject(target, org, app.slug);
}

/**
 * The column names the isolate would have answered under.
 *
 * The isolate returns airlayer's own aliases, `view__member`. The
 * `/semantic-query` route strips that prefix before it answers
 * (`strip_view_prefix`: `view__member` becomes `member` whenever the bare name
 * is unique in the result), so a handler that read `row.region` there would read
 * `row.store__region` once published. Without server code the prefix cannot be
 * asked for, so it is put back from the SPEC: a bare column is qualified when
 * exactly one member the spec asks for (`dimensions`, `measures`,
 * `time_dimensions`) is `<view>.<column>`. A column that is already qualified
 * (the route kept it because of a collision), maps to no member, or maps to
 * several, keeps the route's name.
 */
export function isolateColumnNames(columns: string[], spec: Record<string, unknown>): string[] {
  const names = (value: unknown): string[] =>
    Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : [];
  const timeDimensions = Array.isArray(spec.time_dimensions)
    ? spec.time_dimensions.flatMap((t) =>
        t && typeof t === "object" && typeof (t as { dimension?: unknown }).dimension === "string"
          ? [(t as { dimension: string }).dimension]
          : []
      )
    : [];
  const viewsByMember = new Map<string, Set<string>>();
  for (const field of [...names(spec.dimensions), ...names(spec.measures), ...timeDimensions]) {
    const dot = field.indexOf(".");
    const view = field.slice(0, dot);
    const member = field.slice(dot + 1);
    if (dot <= 0 || member.includes(".") || view.includes("__") || member.includes("__")) continue;
    viewsByMember.set(member, (viewsByMember.get(member) ?? new Set()).add(view));
  }
  return columns.map((column) => {
    if (column.includes("__")) return column;
    const views = viewsByMember.get(column);
    const [view] = views?.size === 1 ? views : [];
    return view === undefined ? column : `${view}__${column}`;
  });
}

/** `{ columns, rows: [[…]], truncated }` to the `{ rows: [{…}], truncated }` the host ops answer. */
function asHostRows(
  label: string,
  body: string,
  rename: (columns: string[]) => string[] = (columns) => columns
): { rows: Record<string, unknown>[]; truncated: boolean } {
  const parsed = parseJson(body) as
    | { columns?: unknown; rows?: unknown; truncated?: unknown }
    | undefined;
  if (!parsed || !Array.isArray(parsed.columns) || !Array.isArray(parsed.rows)) {
    throw new Error(`${label}: the deployment's answer was not { columns, rows, truncated }`);
  }
  const columns = rename(parsed.columns.map(String));
  const rows = (parsed.rows as unknown[][]).map((cells) =>
    Object.fromEntries(columns.map((name, i) => [name, cells[i]]))
  );
  return { rows, truncated: parsed.truncated === true };
}

/**
 * Check the credential and find the project — both before the handler runs, so
 * a missing login or an unknown app is the command's failure and not an error
 * the handler might catch — then build the transport.
 */
export async function createReadThrough(ctx: Context, app: ReadThroughApp): Promise<ReadThrough> {
  refuseSandboxToken(
    ctx.storedBearer(),
    "use `oxyc fn dev --read-through`",
    "it reads the deployment as a person (oxyc login, or OXY_TOKEN set to a user token); a sandbox agent token does the sandbox loop through its own verbs"
  );
  const target = ctx.target();
  const bearer = await ctx.bearer();
  const project = await resolveProject(ctx, target, app);
  const route = (name: string) => `/api/projects/${encodeURIComponent(project)}/${name}`;

  async function post(label: string, path: string, payload: unknown): Promise<string> {
    const response = await request({
      target,
      path,
      method: "POST",
      body: JSON.stringify(payload),
      bearer
    });
    if (response.status < 200 || response.status >= 300) {
      // A plain Error, not a CliError: this is the handler's call failing,
      // and `devOnce` treats a CliError from inside the handler as the
      // command's own failure.
      const refusal = errorForResponse(response);
      const why = refusal.serverMessage ?? refusal.detail ?? "";
      throw new Error(
        `${label}: ${response.status} ${response.statusText} from ${target}${why ? ` — ${why.slice(0, 500)}` : ""}${
          refusal.hint ? ` (${refusal.hint})` : ""
        }`
      );
    }
    return response.body;
  }

  const ops: Record<ReadThroughOp, HostOpImpl> = {
    query: async (args) =>
      asHostRows("ctx.query", await post("ctx.query", route("query"), { sql: String(args.sql) })),
    "warehouse.query": async (args) =>
      asHostRows(
        "ctx.warehouse.query",
        await post("ctx.warehouse.query", route("query"), {
          sql: String(args.sql),
          database: String(args.database)
        })
      ),
    "semantic.query": async (args) => {
      const spec = (args.spec ?? {}) as Record<string, unknown>;
      return asHostRows(
        "ctx.semantic.query",
        // `v: 1` is the body version the browser hook pins (`parse_versioned_body`).
        await post("ctx.semantic.query", route("semantic-query"), { ...spec, v: 1 }),
        (columns) => isolateColumnNames(columns, spec)
      );
    }
  };

  return {
    target,
    project,
    handles: (op) => (READ_THROUGH_OPS as readonly string[]).includes(op),
    install: (t) => {
      for (const op of READ_THROUGH_OPS) t.override(op, ops[op]);
    }
  };
}

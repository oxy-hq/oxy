// React provider + hooks for customer-app bundles.
//
// Every customer app does the same dance: load the manifest once at
// boot, then fire queries as components mount. The bundle developer
// shouldn't have to thread the resolved manifest through every prop
// or wire a custom context per app — that's what this file is for.
//
// Usage:
//
//   import { OxyAppProvider, useQuery } from "@oxy-hq/sdk";
//
//   function App() {
//     return <OxyAppProvider><Dashboard /></OxyAppProvider>;
//   }
//   function Dashboard() {
//     const { rows, error, loading } = useQuery({ sql: "SELECT 1" });
//     ...
//   }
//
// Loading + error states are per-query so the bundle can render
// per-widget skeletons.

import * as React from "react";
import { type CustomerAppErrorReport, interpretCustomerAppError } from "./errors";
import { interpolateSqlParams } from "./interpolate";
import {
  type LoadManifestOptions,
  loadCustomerAppManifest,
  type ResolvedCustomerAppManifest
} from "./manifest";
import { isSafeLinkHref } from "./markdown";

// ── Context ─────────────────────────────────────────────────────────────────

/**
 * Credentialed fetch wrapper stored in context so `useQuery` can share
 * the same request mechanism without coupling it to the global `fetch`.
 *
 * Sends `credentials: "include"` so the session cookie rides along when
 * the app is served by oxy (in-workspace / admin preview) — that cookie
 * authorizes data calls. For local dev (cross-origin), the
 * `@oxy-hq/vite-plugin` proxy attaches the developer's token. Bundles may
 * override the fetcher for test/proxy environments.
 */
export type AppFetcher = typeof fetch;

function defaultFetcher(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  return fetch(input, { credentials: "include", ...init });
}

interface OxyAppContextValue {
  status: "loading" | "ready" | "error";
  resolved?: ResolvedCustomerAppManifest;
  error?: CustomerAppErrorReport;
  /** Credentialed fetch implementation shared by all hooks. */
  fetcher: AppFetcher;
}

const OxyAppContext = React.createContext<OxyAppContextValue | undefined>(undefined);

export interface OxyAppProviderProps {
  /** Optional manifest load options. Same shape as `loadCustomerAppManifest`. */
  manifestOptions?: LoadManifestOptions;
  /**
   * Rendered while the manifest is loading. Defaults to nothing; pass a
   * spinner if you want one.
   */
  fallback?: React.ReactNode;
  /**
   * Rendered on manifest load failure. Receives the structured error
   * report so the bundle can show its own branded error card. Defaults
   * to a minimal text-only fallback (better than a blank page).
   */
  errorFallback?: (err: CustomerAppErrorReport) => React.ReactNode;
  /**
   * Override the fetch implementation used by all hooks (`useQuery`).
   * Useful for test environments or proxy setups. Defaults to a wrapper
   * that sets `credentials: "include"` on every request.
   */
  fetcher?: AppFetcher;
  children: React.ReactNode;
}

/**
 * Top-level provider. Loads the manifest once on mount; children only
 * render after the manifest is ready (or the error fallback fires).
 */
export function OxyAppProvider(props: OxyAppProviderProps): React.JSX.Element {
  const { manifestOptions, fallback, errorFallback, fetcher: fetcherProp, children } = props;
  // Stable fetcher reference: caller-supplied or the module-level default.
  // We don't put this in state because it should never change after mount
  // (same reasoning as manifestOptions).
  const fetcher = fetcherProp ?? defaultFetcher;
  const [state, setState] = React.useState<OxyAppContextValue>({ status: "loading", fetcher });

  React.useEffect(() => {
    let cancelled = false;
    loadCustomerAppManifest(manifestOptions)
      .then((resolved) => {
        if (!cancelled) setState({ status: "ready", resolved, fetcher });
      })
      .catch((e: unknown) => {
        if (!cancelled) setState({ status: "error", error: interpretCustomerAppError(e), fetcher });
      });
    return () => {
      cancelled = true;
    };
    // `manifestOptions` is treated as stable — changing it mid-flight
    // wouldn't make sense for a manifest load (the URL is baked at
    // build time). `fetcher` is derived from props but also stable;
    // it's included here so biome is satisfied and so the effect does
    // update if a test swaps the fetcher between renders.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [manifestOptions, fetcher]);

  if (state.status === "error" && state.error) {
    const err = state.error;
    return (
      <OxyAppContext.Provider value={state}>
        {errorFallback ? errorFallback(err) : defaultErrorFallback(err)}
      </OxyAppContext.Provider>
    );
  }
  if (state.status === "loading") {
    return <OxyAppContext.Provider value={state}>{fallback ?? null}</OxyAppContext.Provider>;
  }
  return <OxyAppContext.Provider value={state}>{children}</OxyAppContext.Provider>;
}

function defaultErrorFallback(err: CustomerAppErrorReport): React.ReactNode {
  // Intentionally inline-styled with system fonts so it works in any
  // bundle without depending on Tailwind / CSS-in-JS / etc.
  return (
    <div
      style={{
        margin: "2rem auto",
        maxWidth: "640px",
        padding: "1rem",
        border: "1px solid #fca5a5",
        background: "#fee2e2",
        color: "#991b1b",
        borderRadius: "8px",
        fontFamily: "system-ui, -apple-system, sans-serif",
        fontSize: "14px"
      }}
    >
      <div style={{ fontWeight: 600 }}>{err.title}</div>
      <pre style={{ fontSize: "12px", marginTop: "4px" }}>{err.message}</pre>
      <div style={{ marginTop: "12px" }}>
        <strong>What to try:</strong> {err.hint}
      </div>
    </div>
  );
}

// ── Beta-warning helper ─────────────────────────────────────────────────────

/**
 * Emit a one-time `console.warn` the first time a beta hook is
 * used in a given page load. Bundles upgrading from a future GA
 * release won't see the warning; the message lets us flag rough
 * edges without breaking the build.
 */
const _warnedBeta = new Set<string>();
function warnBetaOnce(name: string): void {
  if (_warnedBeta.has(name)) return;
  _warnedBeta.add(name);
  if (typeof console !== "undefined" && typeof console.warn === "function") {
    console.warn(
      `[@oxy-hq/sdk] \`${name}\` is in beta — interface and behavior may change. ` +
        `See https://github.com/oxy-hq/customer-apps for caveats and the migration guide.`
    );
  }
}

// ── Error helpers ───────────────────────────────────────────────────────────

/**
 * Error thrown by all customer-app hooks when an API call returns a
 * non-2xx response. Carries the structured `code` + `hint` the server
 * emits so bundle UIs can render an actionable message instead of
 * "404: { ...json... }".
 *
 * The server contract is documented in
 * `crates/app/src/server/api/projects/agent_ask.rs` and
 * `procedure_run.rs` — both emit `{ message, code?, hint? }` as JSON.
 * Hooks that previously wrapped the raw text in `new Error()` now
 * throw this type instead.
 */
export class OxyApiError extends Error {
  readonly status: number;
  readonly code: string | null;
  readonly hint: string | null;
  constructor(opts: {
    status: number;
    message: string;
    code?: string | null;
    hint?: string | null;
  }) {
    // Build a single multi-line `message` so consumers that just
    // render `.message` still see the hint. Code is included so
    // logs / dev tools can grep for it.
    const base = opts.message || `HTTP ${opts.status}`;
    const code = opts.code ? ` [${opts.code}]` : "";
    const hint = opts.hint ? `\n\n${opts.hint}` : "";
    super(`${base}${code}${hint}`);
    this.name = "OxyApiError";
    this.status = opts.status;
    this.code = opts.code ?? null;
    this.hint = opts.hint ?? null;
  }
}

/**
 * Read a non-2xx response from oxy and return an `OxyApiError`.
 * Parses the JSON envelope when present; falls back to raw text
 * (truncated to 240 chars so a runaway HTML error page doesn't
 * dominate the bundle UI).
 */
async function apiErrorFromResponse(resp: Response): Promise<OxyApiError> {
  let body: unknown;
  let raw = "";
  try {
    raw = await resp.text();
    body = raw ? JSON.parse(raw) : undefined;
  } catch {
    // Non-JSON body — keep `raw` for the fallback path.
  }
  if (body && typeof body === "object") {
    const b = body as { message?: unknown; code?: unknown; hint?: unknown };
    return new OxyApiError({
      status: resp.status,
      message: typeof b.message === "string" ? b.message : `HTTP ${resp.status}`,
      code: typeof b.code === "string" ? b.code : null,
      hint: typeof b.hint === "string" ? b.hint : null
    });
  }
  const snippet = raw.length > 240 ? `${raw.slice(0, 237)}…` : raw;
  return new OxyApiError({
    status: resp.status,
    message: snippet || `HTTP ${resp.status}`
  });
}

// ── Hooks ───────────────────────────────────────────────────────────────────

/**
 * Read the resolved manifest from context. Throws if called outside
 * `<OxyAppProvider>` — that's a programmer error worth surfacing
 * loudly, not silently swallowing.
 */
export function useResolvedManifest(): ResolvedCustomerAppManifest {
  const ctx = React.useContext(OxyAppContext);
  if (!ctx) {
    throw new Error("useResolvedManifest must be called inside <OxyAppProvider>");
  }
  if (ctx.status !== "ready" || !ctx.resolved) {
    throw new Error(
      "useResolvedManifest called before manifest finished loading. " +
        "Use the provider's `fallback` prop to render while loading."
    );
  }
  return ctx.resolved;
}

/**
 * Low-level hook that returns the raw context value (including the
 * fetcher). Prefer `useResolvedManifest` for manifest access; use
 * this only when you need the fetcher or projectId without requiring
 * the manifest to be ready (e.g. inside `useQuery`).
 */
function useOxyApp(): { projectId: string | undefined; fetcher: AppFetcher } {
  const ctx = React.useContext(OxyAppContext);
  if (!ctx) {
    throw new Error("useOxyApp must be called inside <OxyAppProvider>");
  }
  return {
    projectId: ctx.resolved?.projectId,
    fetcher: ctx.fetcher
  };
}

// ── useQuery ────────────────────────────────────────────────────────────────

export interface UseQueryInput {
  sql: string;
  database?: string;
}

export interface UseQueryOpts {
  params?: Record<string, string | number | boolean | null | undefined>;
  /** Set false to skip the request (e.g., waiting on user input). */
  enabled?: boolean;
}

export interface UseQueryResult<Row = Record<string, unknown>> {
  rows: Row[];
  columns: string[];
  loading: boolean;
  error: Error | null;
  refetch: () => void;
}

/**
 * Execute an ad-hoc SQL query against the project linked to this
 * customer app. The query is specified inline by the caller; no
 * manifest declaration is involved.
 *
 * Re-runs whenever `input` or enabled `params` change. Use the
 * `enabled` option to defer the first fetch until required data is
 * available (e.g. a user-supplied filter value).
 */
export function useQuery<Row = Record<string, unknown>>(
  input: UseQueryInput,
  opts: UseQueryOpts = {}
): UseQueryResult<Row> {
  const { projectId, fetcher } = useOxyApp();
  const enabled = opts.enabled !== false;
  // Serialise opts.params so `useMemo` fires only when the values
  // change, not on every render when callers pass a new object literal.
  const paramsKey = JSON.stringify(opts.params);
  // biome-ignore lint/correctness/useExhaustiveDependencies: paramsKey replaces opts.params as the dep
  const sqlWithParams = React.useMemo(
    () => interpolateSqlParams(input.sql, opts.params ?? {}),
    [input.sql, paramsKey]
  );

  const [state, setState] = React.useState<{
    rows: Row[];
    columns: string[];
    loading: boolean;
    error: Error | null;
  }>({
    rows: [],
    columns: [],
    loading: enabled && !!projectId,
    error: null
  });
  const [nonce, setNonce] = React.useState(0);

  // biome-ignore lint/correctness/useExhaustiveDependencies: nonce forces refetch
  React.useEffect(() => {
    if (!enabled || !projectId) {
      setState((s) => (s.loading ? { ...s, loading: false } : s));
      return;
    }
    const ctrl = new AbortController();
    let cancelled = false;
    setState((s) => ({ ...s, loading: true, error: null }));

    const body = JSON.stringify({
      sql: sqlWithParams,
      ...(input.database ? { database: input.database } : {})
    });

    fetcher(`/api/projects/${projectId}/query`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body,
      signal: ctrl.signal
    })
      .then(async (resp) => {
        if (!resp.ok) {
          throw await apiErrorFromResponse(resp);
        }
        return resp.json() as Promise<{ columns: string[]; rows: unknown[][] }>;
      })
      .then(({ columns, rows }) => {
        if (cancelled) return;
        const objects = rows.map(
          (r) => Object.fromEntries(columns.map((c, i) => [c, r[i]])) as Row
        );
        setState({ rows: objects, columns, loading: false, error: null });
      })
      .catch((err) => {
        if (cancelled) return;
        // AbortError is not a real failure — the cleanup cancelled the request.
        if (err instanceof DOMException && err.name === "AbortError") return;
        setState((s) => ({
          ...s,
          loading: false,
          error: err instanceof Error ? err : new Error(String(err))
        }));
      });

    return () => {
      cancelled = true;
      ctrl.abort();
    };
  }, [enabled, projectId, sqlWithParams, input.database, nonce, fetcher]);

  return {
    rows: state.rows,
    columns: state.columns,
    loading: state.loading,
    error: state.error,
    refetch: () => setNonce((n) => n + 1)
  };
}

// ── useSemanticQuery ────────────────────────────────────────────────────────
//
// Bundles reference the project's semantic layer by topic + dimensions
// + measures + filters. The server compiles to dialect-specific SQL via
// airlayer and executes through the same connector path as useQuery —
// when the data team refactors the SQL behind a measure, the bundle
// picks up the change without an edit.

/** Scalar filter operators (compared against a single value). */
export type SemanticScalarOp = "eq" | "neq" | "lt" | "lte" | "gt" | "gte";

/** Array filter operators (compared against a list). */
export type SemanticArrayOp = "in" | "not_in";

/** Date-range filter operators. `from` / `to` accept ISO date strings. */
export type SemanticDateRangeOp = "in_date_range" | "not_in_date_range";

/**
 * One filter clause. The `field` references a dimension name within
 * the topic; the `op` discriminator picks which other fields are
 * meaningful. Wire shape matches `agentic_semantic::SemanticFilter`
 * verbatim — the bundle's request body is forwarded to airlayer's
 * compiler with no translation.
 */
export type SemanticFilter =
  | { field: string; op: SemanticScalarOp; value: string | number | boolean | null }
  | { field: string; op: SemanticArrayOp; values: Array<string | number | boolean | null> }
  | { field: string; op: SemanticDateRangeOp; from: string; to: string };

/** Time dimensions with optional granularity (e.g. "day", "month"). */
export interface SemanticTimeDimension {
  dimension: string;
  granularity?: "day" | "week" | "month" | "quarter" | "year";
}

export interface UseSemanticQueryInput {
  topic: string;
  dimensions?: string[];
  measures?: string[];
  time_dimensions?: SemanticTimeDimension[];
  filters?: SemanticFilter[];
  limit?: number;
}

export interface UseSemanticQueryOpts {
  /** Set false to skip the request (e.g., waiting on user input). */
  enabled?: boolean;
  /**
   * When true, the response includes the compiled SQL string at
   * `sql`. Off by default — production callers shouldn't bake the
   * warehouse SQL into their UI. Bundle authors flip this on while
   * debugging.
   */
  debug?: boolean;
}

export interface UseSemanticQueryResult<Row = Record<string, unknown>> {
  rows: Row[];
  columns: string[];
  /** True when the result was capped at the server's row limit. */
  truncated: boolean;
  /** Compiled SQL — populated only when `opts.debug` is true. */
  sql: string | null;
  loading: boolean;
  error: Error | null;
  refetch: () => void;
}

/**
 * Run a semantic-layer query against the project's `.view.yml` /
 * `.topic.yml` definitions. The server compiles to SQL and executes
 * through the same connector path as `useQuery`, so result shape
 * matches.
 *
 * Re-runs whenever the input shape changes (deep-compared via JSON).
 * Use `opts.enabled = false` to defer the first fetch until required
 * inputs (e.g. a user-picked filter value) are available.
 */
export function useSemanticQuery<Row = Record<string, unknown>>(
  input: UseSemanticQueryInput,
  opts: UseSemanticQueryOpts = {}
): UseSemanticQueryResult<Row> {
  const { projectId, fetcher } = useOxyApp();
  const enabled = opts.enabled !== false;
  const debug = opts.debug === true;

  // Stable key for the effect dep — re-running on every render when
  // callers pass a new object literal would mean re-fetching on every
  // parent render.
  const inputKey = React.useMemo(() => JSON.stringify(input), [input]);

  const [state, setState] = React.useState<{
    rows: Row[];
    columns: string[];
    truncated: boolean;
    sql: string | null;
    loading: boolean;
    error: Error | null;
  }>({
    rows: [],
    columns: [],
    truncated: false,
    sql: null,
    loading: enabled && !!projectId,
    error: null
  });
  const [nonce, setNonce] = React.useState(0);

  // biome-ignore lint/correctness/useExhaustiveDependencies: inputKey serializes input; nonce forces refetch
  React.useEffect(() => {
    if (!enabled || !projectId) {
      setState((s) => (s.loading ? { ...s, loading: false } : s));
      return;
    }
    const ctrl = new AbortController();
    let cancelled = false;
    setState((s) => ({ ...s, loading: true, error: null }));

    // `v: 1` is the customer-apps-platform body versioning convention
    // (see customer_apps_gates.rs::parse_versioned_body). Pinning it
    // here means a future v2 server can reject this stale client
    // cleanly instead of silently misinterpreting it.
    const body = JSON.stringify({
      v: 1,
      topic: input.topic,
      dimensions: input.dimensions ?? [],
      measures: input.measures ?? [],
      time_dimensions: input.time_dimensions ?? [],
      filters: input.filters ?? [],
      ...(input.limit != null ? { limit: input.limit } : {})
    });

    const url = `/api/projects/${projectId}/semantic-query${debug ? "?debug=1" : ""}`;
    fetcher(url, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body,
      signal: ctrl.signal
    })
      .then(async (resp) => {
        if (!resp.ok) {
          throw await apiErrorFromResponse(resp);
        }
        return resp.json() as Promise<{
          columns: string[];
          rows: unknown[][];
          truncated: boolean;
          sql?: string;
        }>;
      })
      .then(({ columns, rows, truncated, sql }) => {
        if (cancelled) return;
        const objects = rows.map(
          (r) => Object.fromEntries(columns.map((c, i) => [c, r[i]])) as Row
        );
        setState({
          rows: objects,
          columns,
          truncated,
          sql: sql ?? null,
          loading: false,
          error: null
        });
      })
      .catch((err) => {
        if (cancelled) return;
        if (err instanceof DOMException && err.name === "AbortError") return;
        setState((s) => ({
          ...s,
          loading: false,
          error: err instanceof Error ? err : new Error(String(err))
        }));
      });

    return () => {
      cancelled = true;
      ctrl.abort();
    };
  }, [enabled, projectId, inputKey, debug, nonce, fetcher]);

  return {
    rows: state.rows,
    columns: state.columns,
    truncated: state.truncated,
    sql: state.sql,
    loading: state.loading,
    error: state.error,
    refetch: () => setNonce((n) => n + 1)
  };
}

// ── useProcedureRun ─────────────────────────────────────────────────────────
//
// Trigger a long-running procedure (`.procedure.yml`) from the
// bundle. Returns state + progress + structured outputs. Use for
// "Generate report" / "Recompute" buttons.

export type ProcedureRunState = "idle" | "running" | "done" | "failed";

export interface UseProcedureRunInput {
  procedureId: string;
}

export interface UseProcedureRunOpts {
  /** Polling cadence in ms while running. Default: 2000 (procedures
   *  are typically minutes-long; tighter cadence wastes resources). */
  pollIntervalMs?: number;
  pollIntervalBackoffMs?: number;
  /** Max client-side wait in ms. Default: 1 hour. */
  maxWaitMs?: number;
}

export interface ProcedureProgress {
  step: string;
  percent: number;
}

export interface ProcedureResult {
  summary: string;
  outputs: Record<string, unknown>;
}

export interface UseProcedureRunResult {
  state: ProcedureRunState;
  run: (params?: Record<string, unknown>) => void;
  /** Cancel the in-flight run. Idempotent. */
  cancel: () => void;
  progress: ProcedureProgress | null;
  result: ProcedureResult | null;
  error: Error | null;
}

const PROCEDURE_POLL_MS = 2000;
const PROCEDURE_BACKOFF_MS = 5000;
const PROCEDURE_MAX_WAIT_MS = 60 * 60 * 1000;

/**
 * @beta Long-running procedure runner. The wire shape works end-to-end
 * (start → poll → cancel; runs survive server restarts via the
 * `customer_app_procedure_runs` table) but a few rough edges remain
 * before this is GA-ready:
 *
 *   - Hint surfaces for `procedure_not_found` are correct but the
 *     procedure-discovery rules (which directories the server scans,
 *     case-sensitivity, branch awareness) aren't documented yet.
 *   - Cancellation across multi-instance deployments leans on a
 *     periodic sweep — fine for now, but expect occasional latency
 *     between `cancel()` and the run actually stopping.
 *   - Progress reporting requires the procedure to emit named
 *     steps; bundles get `progress: null` until that lands.
 *
 * The API surface is stable; expect breaking changes only if the
 * server-side `customer_app_procedure_runs` schema changes.
 */
export function useProcedureRun(
  input: UseProcedureRunInput,
  opts: UseProcedureRunOpts = {}
): UseProcedureRunResult {
  warnBetaOnce("useProcedureRun");
  const { projectId, fetcher } = useOxyApp();
  const pollMs = opts.pollIntervalMs ?? PROCEDURE_POLL_MS;
  const backoffMs = opts.pollIntervalBackoffMs ?? PROCEDURE_BACKOFF_MS;
  const maxWaitMs = opts.maxWaitMs ?? PROCEDURE_MAX_WAIT_MS;

  const [state, setState] = React.useState<{
    state: ProcedureRunState;
    progress: ProcedureProgress | null;
    result: ProcedureResult | null;
    error: Error | null;
  }>({
    state: "idle",
    progress: null,
    result: null,
    error: null
  });
  const inflight = React.useRef<{
    abort?: AbortController;
    runId?: string;
  }>({});

  // cancel is idempotent and a no-op once the run has reached a
  // terminal state. inflight.current.runId is cleared in the same
  // setState calls that flip state out of "running"; that's the
  // single source of truth so a slow click after `done` can't
  // overwrite the result with a phantom "failed".
  const cancel = React.useCallback(() => {
    const runId = inflight.current.runId;
    if (!projectId || !runId) return;
    inflight.current.abort?.abort();
    inflight.current.runId = undefined;
    void fetcher(`/api/projects/${projectId}/procedures/runs/${encodeURIComponent(runId)}/cancel`, {
      method: "POST"
    }).catch(() => {});
    setState({
      state: "failed",
      progress: null,
      result: null,
      error: new Error("procedure cancelled by user")
    });
  }, [projectId, fetcher]);

  const run = React.useCallback(
    (params?: Record<string, unknown>) => {
      if (!projectId) {
        setState((s) => ({
          ...s,
          state: "failed",
          error: new Error("project not configured")
        }));
        return;
      }
      inflight.current.abort?.abort();
      const ctrl = new AbortController();
      inflight.current = { abort: ctrl };
      setState({ state: "running", progress: null, result: null, error: null });

      void (async () => {
        try {
          const body = JSON.stringify({
            v: 1,
            ...(params ? { params } : {})
          });
          const startResp = await fetcher(
            `/api/projects/${projectId}/procedures/${encodeURIComponent(input.procedureId)}/runs`,
            {
              method: "POST",
              headers: { "content-type": "application/json" },
              body,
              signal: ctrl.signal
            }
          );
          if (!startResp.ok) {
            throw await apiErrorFromResponse(startResp);
          }
          const { run_id } = (await startResp.json()) as { run_id: string };
          inflight.current.runId = run_id;

          const startedAt = Date.now();
          let pollCount = 0;
          // biome-ignore lint/correctness/noConstantCondition: terminated by return/throw
          while (true) {
            if (ctrl.signal.aborted) return;
            if (Date.now() - startedAt > maxWaitMs) {
              throw new Error("procedure run timed out client-side");
            }
            const interval = pollCount < 6 ? pollMs : backoffMs;
            await sleep(interval, ctrl.signal);
            if (ctrl.signal.aborted) return;
            pollCount += 1;

            const pollResp = await fetcher(
              `/api/projects/${projectId}/procedures/runs/${encodeURIComponent(run_id)}`,
              { method: "GET", signal: ctrl.signal }
            );
            if (!pollResp.ok) {
              throw await apiErrorFromResponse(pollResp);
            }
            const poll = (await pollResp.json()) as
              | { status: "running"; progress?: ProcedureProgress }
              | { status: "done"; result: ProcedureResult }
              | { status: "cancelled" }
              | {
                  status: "failed";
                  error: { message: string; code?: string };
                };
            if (poll.status === "running") {
              if (poll.progress) {
                setState((s) => ({ ...s, progress: poll.progress ?? null }));
              }
              continue;
            }
            if (poll.status === "done") {
              inflight.current.runId = undefined;
              setState({
                state: "done",
                progress: null,
                result: poll.result,
                error: null
              });
              return;
            }
            if (poll.status === "cancelled") {
              inflight.current.runId = undefined;
              setState({
                state: "failed",
                progress: null,
                result: null,
                error: new Error("procedure cancelled")
              });
              return;
            }
            inflight.current.runId = undefined;
            setState({
              state: "failed",
              progress: null,
              result: null,
              error: new Error(poll.error.message)
            });
            return;
          }
        } catch (e) {
          if (e instanceof DOMException && e.name === "AbortError") return;
          inflight.current.runId = undefined;
          setState({
            state: "failed",
            progress: null,
            result: null,
            error: e instanceof Error ? e : new Error(String(e))
          });
        }
      })();
    },
    [projectId, fetcher, input.procedureId, pollMs, backoffMs, maxWaitMs]
  );

  React.useEffect(() => {
    return () => {
      inflight.current.abort?.abort();
    };
  }, []);

  return {
    state: state.state,
    run,
    cancel,
    progress: state.progress,
    result: state.result,
    error: state.error
  };
}

// ── useAgentRun (SSE streaming) ─────────────────────────────────────────────
//
// Real-time chat surface. Agentic pipeline emits events as they
// happen — token-by-token answer text, mid-run SQL artifacts, ask-
// user clarifications. Bundles use this for any chat / Q&A UI; the
// drop-in `<OxyChat>` and `<OxyAnswer>` components wrap it for the
// common cases.

export type AgentRunState = "idle" | "running" | "needs_clarification" | "done" | "failed";

export interface AgentRunEvent {
  type: string;
  data: unknown;
}

/** SQL produced and (optionally) executed by the agent. Extracted
 *  from `query_generated` / `query_executed` / `verified_sql` /
 *  `semantic_query` / `omni_query` SSE events so callers don't have
 *  to scan the raw event stream themselves. */
export interface AgentSqlArtifact {
  type: "sql";
  /** Stable id derived from the SSE event id so React keys stay
   *  stable across re-renders / reconnects. */
  id: string;
  /** Originating UI event type — preserves the verified/semantic/etc.
   *  flavor in case the renderer wants a badge. */
  source: string;
  sql: string;
  /** Present when the SQL was executed and rows came back. */
  results?: {
    columns: string[];
    rows: unknown[][];
    rowCount: number;
  };
  /** Present when execution failed — surface it so the bundle UI can
   *  show the failure inline next to the SQL instead of swallowing
   *  it inside the agent's final answer. */
  error?: string;
}

export type AgentArtifact = AgentSqlArtifact;

export interface UseAgentRunInput {
  agentId: string;
}

export interface UseAgentRunResult {
  state: AgentRunState;
  /** Submit a question and open the SSE stream. */
  ask: (question: string, opts?: { threadId?: string }) => void;
  /** Cancel the in-flight stream + the server-side run. Idempotent. */
  cancel: () => void;
  /** Accumulated raw events for advanced consumers. */
  events: AgentRunEvent[];
  /** SQL artifacts extracted from the event stream — convenience
   *  view over `events` so renderers don't have to know which event
   *  types carry SQL. */
  artifacts: AgentArtifact[];
  /** Final answer once a `done` event arrives. Markdown. */
  answer: string | null;
  /** Clarification text once a suspension event arrives. */
  clarification: string | null;
  /** Thread id used by the active run (stable across follow-ups). */
  threadId: string | null;
  /**
   * @beta Relative path to the full thread view in oxy (e.g.
   * `/threads/<id>` for local mode, or
   * `/<org_slug>/workspaces/<ws_id>/threads/<id>` in cloud). Set
   * once the run starts so a bundle can render a "Continue in Oxy"
   * link without constructing the URL itself.
   *
   * Caveats while in beta:
   *   - The bundle's origin and the oxy app shell's origin can
   *     differ in cloud deployments. If they do, this relative URL
   *     resolves against the bundle's origin and 404s. A future
   *     release will expose the oxy app origin via the manifest;
   *     for now, prefix at the call site if you know your
   *     deployment topology, or hide the link entirely.
   *   - The thread row may not be queryable until the run produces
   *     its first event — clicking the link immediately after
   *     `ask()` can land on a "thread not found" page.
   */
  threadUrl: string | null;
  error: Error | null;
}

export function useAgentRun(input: UseAgentRunInput): UseAgentRunResult {
  const { projectId, fetcher } = useOxyApp();
  const [state, setState] = React.useState<{
    state: AgentRunState;
    events: AgentRunEvent[];
    artifacts: AgentArtifact[];
    answer: string | null;
    clarification: string | null;
    threadId: string | null;
    threadUrl: string | null;
    error: Error | null;
  }>({
    state: "idle",
    events: [],
    artifacts: [],
    answer: null,
    clarification: null,
    threadId: null,
    threadUrl: null,
    error: null
  });
  // Track the latest abort controller + run_id so cancel() can fire
  // the right server endpoint AND tear down the in-flight stream.
  // `terminated` is a local-only flag the consume loop reads to
  // decide whether to reconnect after a clean stream close. We
  // can't read React state directly (closure capture is stale) and
  // the prior stateRef approach was racy because setState is async
  // — by the time we read the ref it might not have flushed yet.
  // The flag is set synchronously inside the onEvent handler right
  // before setState fires, so the consume loop sees it on the next
  // iteration.
  const inflight = React.useRef<{
    abort?: AbortController;
    runId?: string;
  }>({});

  // cancel is idempotent + no-op once the run is terminal. We
  // clear inflight.current.runId in the same setState calls that
  // flip state out of "running"; a late click then can't overwrite
  // a done answer with a phantom "failed".
  const cancel = React.useCallback(() => {
    const runId = inflight.current.runId;
    if (!projectId || !runId) return;
    inflight.current.abort?.abort();
    inflight.current.runId = undefined;
    void fetcher(`/api/projects/${projectId}/agents/asks/${encodeURIComponent(runId)}/cancel`, {
      method: "POST"
    }).catch(() => {});
    setState((s) => ({
      ...s,
      state: "failed",
      error: new Error("agent run cancelled by user")
    }));
  }, [projectId, fetcher]);

  const ask = React.useCallback(
    (question: string, opts: { threadId?: string } = {}) => {
      if (!projectId) {
        setState((s) => ({
          ...s,
          state: "failed",
          error: new Error("project not configured")
        }));
        return;
      }
      inflight.current.abort?.abort();
      const ctrl = new AbortController();
      inflight.current = { abort: ctrl };
      setState({
        state: "running",
        events: [],
        artifacts: [],
        answer: null,
        clarification: null,
        threadId: opts.threadId ?? null,
        threadUrl: opts.threadId ? `/threads/${opts.threadId}` : null,
        error: null
      });

      void (async () => {
        try {
          // 1. POST to start the run.
          const body = JSON.stringify({
            v: 1,
            question,
            ...(opts.threadId ? { thread_id: opts.threadId } : {})
          });
          const startResp = await fetcher(
            `/api/projects/${projectId}/agents/${encodeURIComponent(input.agentId)}/asks`,
            {
              method: "POST",
              headers: { "content-type": "application/json" },
              body,
              signal: ctrl.signal
            }
          );
          if (!startResp.ok) {
            throw await apiErrorFromResponse(startResp);
          }
          const { run_id, thread_id, thread_url } = (await startResp.json()) as {
            run_id: string;
            thread_id: string;
            thread_url?: string;
          };
          inflight.current.runId = run_id;
          setState((s) => ({
            ...s,
            threadId: thread_id,
            threadUrl: thread_url ?? `/threads/${thread_id}`
          }));

          // 2. Open the SSE stream via fetch (not EventSource) so we
          //    can pass `Last-Event-ID` on reconnect. EventSource has
          //    no API to set that header — the browser exposes the
          //    server-set id internally but doesn't surface it for
          //    custom auth flows or `withCredentials` + custom
          //    headers together. Hand-rolled SSE + ReadableStream
          //    gives us both.
          //
          //    Reconnect strategy: on disconnect (network error,
          //    server close before terminal event), wait 1s and
          //    re-open with the last-seen sequence id. Max 5
          //    attempts so a permanently-broken stream eventually
          //    surfaces as `failed`. The `terminated` flag is set
          //    synchronously when a terminal event arrives — once
          //    set, the loop exits on the next iteration without
          //    reconnecting.
          let lastEventId = "";
          let attempts = 0;
          let terminated = false;
          // biome-ignore lint/correctness/noConstantCondition: terminated by return/break
          while (true) {
            if (ctrl.signal.aborted) return;
            if (terminated) return;
            attempts += 1;

            try {
              await consumeSseStream({
                url: `/api/projects/${projectId}/agents/runs/${encodeURIComponent(run_id)}/events`,
                fetcher,
                signal: ctrl.signal,
                lastEventId,
                onEvent: (ev) => {
                  // Track last event id for reconnect resumption.
                  if (ev.id) lastEventId = ev.id;
                  const data = parseSseData(ev.data);
                  const eventType = ev.event || "message";
                  const artifact = extractSqlArtifact(eventType, ev.id, data);

                  // `text_delta` carries the streaming answer token-by-
                  // token (CoreEvent::LlmToken → UiBlock::TextDelta).
                  // The terminal `done` event has an empty payload — it
                  // signals the run is over but doesn't restate the
                  // answer text. So we accumulate tokens here.
                  const token =
                    eventType === "text_delta" &&
                    typeof data === "object" &&
                    data !== null &&
                    "token" in data
                      ? String((data as { token: unknown }).token)
                      : null;

                  setState((s) => ({
                    ...s,
                    events: [...s.events, { type: eventType, data }],
                    artifacts: artifact ? [...s.artifacts, artifact] : s.artifacts,
                    answer: token !== null ? (s.answer ?? "") + token : s.answer
                  }));

                  if (ev.event === "done") {
                    terminated = true;
                    inflight.current.runId = undefined;
                    setState((s) => ({ ...s, state: "done" }));
                  } else if (
                    ev.event === "failed" ||
                    ev.event === "error" ||
                    ev.event === "cancelled"
                  ) {
                    // Mirrors `is_terminal_event` in
                    // crates/app/.../agent_run_stream.rs — keep in sync.
                    terminated = true;
                    inflight.current.runId = undefined;
                    const message =
                      typeof data === "object" && data !== null && "message" in data
                        ? String((data as { message: unknown }).message)
                        : `agent run ${ev.event}`;
                    setState((s) => ({
                      ...s,
                      state: "failed",
                      error: new Error(message)
                    }));
                  } else if (ev.event === "ask_user") {
                    // Not terminal in the cancel-sense — the user
                    // can resume by calling ask() again with the
                    // same threadId. Keep runId set so cancel()
                    // still works if the user prefers to abort.
                    terminated = true;
                    const clarification =
                      typeof data === "object" && data !== null && "question" in data
                        ? String((data as { question: unknown }).question)
                        : "Agent needs clarification.";
                    setState((s) => ({
                      ...s,
                      state: "needs_clarification",
                      clarification
                    }));
                  }
                }
              });
              // Stream closed cleanly. `terminated` was flipped by
              // the event handler iff a terminal event arrived —
              // check it at loop head; otherwise fall through to
              // reconnect.
            } catch (err) {
              if (err instanceof DOMException && err.name === "AbortError") return;
              // Network / parse error. Reconnect up to 5x with
              // 1s sleep before giving up.
              if (attempts >= 5) {
                inflight.current.runId = undefined;
                setState((s) => ({
                  ...s,
                  state: "failed",
                  error: err instanceof Error ? err : new Error(String(err))
                }));
                return;
              }
            }
            if (terminated) return;
            await sleep(1000, ctrl.signal);
          }
        } catch (e) {
          if (e instanceof DOMException && e.name === "AbortError") return;
          inflight.current.runId = undefined;
          setState((s) => ({
            ...s,
            state: "failed",
            error: e instanceof Error ? e : new Error(String(e))
          }));
        }
      })();
    },
    [projectId, fetcher, input.agentId]
  );

  React.useEffect(() => {
    return () => {
      inflight.current.abort?.abort();
    };
  }, []);

  return {
    state: state.state,
    ask,
    cancel,
    events: state.events,
    artifacts: state.artifacts,
    answer: state.answer,
    clarification: state.clarification,
    threadId: state.threadId,
    threadUrl: state.threadUrl,
    error: state.error
  };
}

/** Parsed JSON payload of an SSE `data:` line, falling back to raw
 *  string on parse failure. */
function parseSseData(raw: string): unknown {
  try {
    return JSON.parse(raw);
  } catch {
    return raw;
  }
}

/** UI event types in the analytics taxonomy that carry SQL the bundle
 *  may want to render alongside the answer. Each carries the same
 *  shape (`query` / `columns` / `rows` / `success`) so we can parse
 *  uniformly. New types added upstream don't surface as artifacts
 *  until added here — that's intentional, the renderer needs to
 *  know how to display each. */
const SQL_EVENT_TYPES = new Set([
  "query_executed",
  "query_generated",
  "verified_sql",
  "semantic_query",
  "omni_query"
]);

/** Extract a SQL artifact from a single SSE event when the type +
 *  payload shape match. Returns `null` for events that aren't SQL
 *  carriers or whose payload doesn't include the expected fields. */
function extractSqlArtifact(
  eventType: string,
  eventId: string,
  data: unknown
): AgentSqlArtifact | null {
  if (!SQL_EVENT_TYPES.has(eventType)) return null;
  if (typeof data !== "object" || data === null) return null;
  const obj = data as Record<string, unknown>;
  const sql =
    typeof obj.query === "string" ? obj.query : typeof obj.sql === "string" ? obj.sql : null;
  if (!sql) return null;

  const columns = Array.isArray(obj.columns) ? (obj.columns as unknown[]).map(String) : undefined;
  const rows = Array.isArray(obj.rows) ? (obj.rows as unknown[][]) : undefined;
  const rowCount =
    typeof obj.row_count === "number"
      ? obj.row_count
      : typeof obj.rowCount === "number"
        ? obj.rowCount
        : rows?.length;

  const artifact: AgentSqlArtifact = {
    type: "sql",
    id: eventId || `${eventType}-${sql.slice(0, 32)}`,
    source: eventType,
    sql
  };
  if (columns && rows) {
    artifact.results = { columns, rows, rowCount: rowCount ?? rows.length };
  }
  const errMsg =
    typeof obj.error === "string"
      ? obj.error
      : obj.success === false && typeof obj.message === "string"
        ? (obj.message as string)
        : undefined;
  if (errMsg) artifact.error = errMsg;
  return artifact;
}

/** One delivered SSE event. Matches what the spec calls a "message"
 *  block — id + event-type + accumulated data. */
interface ParsedSseEvent {
  id: string;
  event: string;
  data: string;
}

/**
 * Hand-rolled SSE consumer using `fetch` + `ReadableStream`. We use
 * this in place of `EventSource` because:
 *   1. EventSource doesn't expose the connection's `Last-Event-ID`
 *      header in a way you can control. The browser tracks it
 *      internally but you can't pass a starting value, so a hook
 *      that wants to resume after a tab switch / network blip has
 *      no way to ask the server to replay from a known point.
 *   2. EventSource can't pass `Authorization` / other custom
 *      headers — only `withCredentials` for cookies. Fine today,
 *      but couples us to cookie auth forever.
 *
 * The parser handles the message-block model from the SSE spec
 * verbatim: lines split by `\n` (or `\r\n` / `\r`), event blocks
 * separated by blank lines, `id:` / `event:` / `data:` fields
 * accumulated per block. Multiple `data:` lines concatenate with
 * `\n` (per spec) — we honor that even though the server emits
 * single-line data today.
 *
 * Throws on network error or non-2xx. Returns when the stream ends
 * normally (server closed connection cleanly).
 */
async function consumeSseStream(opts: {
  url: string;
  fetcher: AppFetcher;
  signal: AbortSignal;
  lastEventId: string;
  onEvent: (ev: ParsedSseEvent) => void;
}): Promise<void> {
  const headers: Record<string, string> = {
    accept: "text/event-stream",
    "cache-control": "no-cache"
  };
  if (opts.lastEventId) {
    headers["Last-Event-ID"] = opts.lastEventId;
  }
  const resp = await opts.fetcher(opts.url, {
    method: "GET",
    headers,
    signal: opts.signal
  });
  if (!resp.ok) {
    throw await apiErrorFromResponse(resp);
  }
  if (!resp.body) {
    throw new Error("SSE response has no body");
  }

  const reader = resp.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let currentId = "";
  let currentEvent = "message";
  let currentData: string[] = [];

  const dispatch = () => {
    if (currentData.length === 0 && currentEvent === "message" && !currentId) {
      return; // empty block
    }
    opts.onEvent({
      id: currentId,
      event: currentEvent,
      data: currentData.join("\n")
    });
    // `id` persists across events per spec; `event` and `data` reset.
    currentEvent = "message";
    currentData = [];
  };

  // biome-ignore lint/correctness/noConstantCondition: terminated by reader.read()
  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });

    let nlIndex: number;
    // Process complete lines. Handle \n, \r\n, and bare \r.
    // biome-ignore lint/suspicious/noAssignInExpressions: idiomatic line-buffer drain
    while ((nlIndex = buffer.search(/\r\n|\r|\n/)) !== -1) {
      // \r at the very end of the buffer is ambiguous — it could be
      // a lone CR (Mac line ending) or the first half of a CRLF
      // whose LF hasn't arrived yet. Wait for the next read before
      // committing. Without this, a CRLF straddling a chunk boundary
      // gets parsed as two empty lines, which fires `dispatch()`
      // twice and breaks event framing for any future writer that
      // emits CRLF (Axum uses LF today, so latent — but bug-class
      // fix is cheap.)
      if (nlIndex === buffer.length - 1 && buffer[nlIndex] === "\r") {
        break;
      }
      const line = buffer.slice(0, nlIndex);
      // Skip the matched line break (one or two chars).
      const sep = buffer.slice(nlIndex, nlIndex + 2);
      buffer = buffer.slice(nlIndex + (sep === "\r\n" ? 2 : 1));

      if (line === "") {
        // Empty line ⇒ end of event block.
        dispatch();
        continue;
      }
      if (line.startsWith(":")) {
        // Comment — keep-alive heartbeats. Skip.
        continue;
      }
      const colonAt = line.indexOf(":");
      const field = colonAt === -1 ? line : line.slice(0, colonAt);
      let value = colonAt === -1 ? "" : line.slice(colonAt + 1);
      if (value.startsWith(" ")) value = value.slice(1);

      switch (field) {
        case "id":
          currentId = value;
          break;
        case "event":
          currentEvent = value;
          break;
        case "data":
          currentData.push(value);
          break;
        // `retry:` and unknown fields ignored.
      }
    }
  }
  // Stream ended — dispatch any final buffered event without a
  // trailing blank line (server didn't write one before close).
  if (currentData.length > 0) {
    dispatch();
  }
}

function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    const t = setTimeout(resolve, ms);
    signal?.addEventListener(
      "abort",
      () => {
        clearTimeout(t);
        reject(new DOMException("aborted", "AbortError"));
      },
      { once: true }
    );
  });
}

// ── useTrackEvent (custom-app usage tracking) ────────────────────────────────

/**
 * Engineer-tagged usage event. Free-form `event_name` (≤ 64 chars,
 * `[a-z][a-z0-9-]*` validated server-side) + optional JSON `payload`
 * (object, ≤ 4 KiB serialized). Surfaces in the admin Activity tab
 * grouped by name, with drill-down into recent occurrences.
 *
 * The handler returned by [`useTrackEvent`] is **fire-and-forget**:
 * it enqueues the event into an in-memory batch flushed every second
 * (and on `pagehide` so a navigation away doesn't drop the tail).
 * No await semantics — call it inline from a click handler without
 * awaiting it. Server-side validation errors are logged to the
 * console; the call site doesn't need to handle them.
 *
 * Example:
 * ```tsx
 * const track = useTrackEvent();
 * <button
 *   onClick={() => {
 *     track("export-clicked", { format: "csv", rowCount });
 *     doExport();
 *   }}
 * >Export</button>
 * ```
 *
 * Rate-limited at 60/min per (user, app) on the server. A burst that
 * trips the limit drops the excess events with a console warning;
 * within-limit events are unaffected.
 */
export function useTrackEvent(): (name: string, payload?: Record<string, unknown>) => void {
  const { projectId, fetcher } = useOxyApp();
  // Per-mount queue. Held in a ref so callers can fire from event
  // handlers without re-rendering. Flush schedules itself once a
  // queued event exists; the cleanup on unmount drains synchronously
  // via the `pagehide` listener.
  const queueRef = React.useRef<Array<{ event_name: string; payload: Record<string, unknown> }>>(
    []
  );
  const flushTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(null);

  const flush = React.useCallback(() => {
    flushTimerRef.current = null;
    if (!projectId) return;
    const batch = queueRef.current;
    if (batch.length === 0) return;
    queueRef.current = [];
    // The server endpoint takes one event per request — keep the
    // wire format simple, fire each in parallel. With the 60/min
    // server-side rate limit + the engineer-tagged surface area
    // (one-call-per-meaningful-interaction), batch sizes are tiny.
    for (const evt of batch) {
      const url = `/api/customer-apps/${projectId}/events`;
      // Use sendBeacon when the page is unloading — keeps the request
      // alive past navigation. Otherwise normal fetch via the
      // context-provided wrapper (which includes credentials + the
      // engineer's bearer when running cross-origin in `pnpm dev`).
      //
      // Dev-mode gap: `navigator.sendBeacon` is a fixed browser API
      // that carries the user's cookies but doesn't go through the
      // OxyAppProvider `fetcher` wrapper, so the `OXY_TOKEN` bearer
      // the vite-plugin proxy adds in cross-origin `pnpm dev` is
      // missing on these requests. Result: a click that fires + the
      // tab closes immediately in local dev gets dropped at the
      // gate as 401. Same-origin prod (cookie auth) is unaffected
      // because the cookie travels with sendBeacon.
      const body = JSON.stringify(evt);
      try {
        if (
          typeof navigator !== "undefined" &&
          typeof navigator.sendBeacon === "function" &&
          document.visibilityState === "hidden"
        ) {
          navigator.sendBeacon(url, new Blob([body], { type: "application/json" }));
        } else {
          fetcher(url, {
            method: "POST",
            headers: { "content-type": "application/json" },
            body
          }).catch((e: unknown) => {
            // Server-side validation errors / rate-limit hits land
            // here. Surface in console so engineers can see them
            // during dev without disrupting the app.
            // biome-ignore lint/suspicious/noConsole: tracking is dev-visible diagnostic
            console.warn("[oxy] useTrackEvent flush failed:", e);
          });
        }
      } catch (e) {
        // biome-ignore lint/suspicious/noConsole: see above
        console.warn("[oxy] useTrackEvent enqueue failed:", e);
      }
    }
  }, [projectId, fetcher]);

  // Drain on page hide / unload so a "click then immediately navigate
  // away" doesn't lose the click. Cleanup also clears any pending
  // 1s flush timer — without this, the timer keeps a callback on an
  // empty queue alive after unmount (harmless but untidy).
  React.useEffect(() => {
    if (typeof window === "undefined") return;
    const onHide = () => flush();
    window.addEventListener("pagehide", onHide);
    return () => {
      window.removeEventListener("pagehide", onHide);
      flush();
      if (flushTimerRef.current !== null) {
        clearTimeout(flushTimerRef.current);
        flushTimerRef.current = null;
      }
    };
  }, [flush]);

  return React.useCallback(
    (name: string, payload?: Record<string, unknown>) => {
      queueRef.current.push({ event_name: name, payload: payload ?? {} });
      if (flushTimerRef.current === null) {
        flushTimerRef.current = setTimeout(flush, 1000);
      }
    },
    [flush]
  );
}

// ── <OxyAnswer> + <OxyChat> drop-in components ──────────────────────────────
//
// Bundles that want a chat surface without rolling their own UI use
// `<OxyChat agentId="...">`. Bundles that already have a question
// input but want oxy to render the answer use `<OxyAnswer>` with the
// values from `useAgentRun()`.
//
// Both components ship with default markdown rendering, SQL artifact
// display, and a "Continue in Oxy" link — the three things every
// bundle author needs and nobody wants to rebuild.
//
// Styling: inline `style={}` only. Bundles inevitably have their
// own design system (Tailwind, Mantine, CSS-in-JS, etc.) and we
// don't want the SDK to fight it. Caller can pass `className` to
// override layout entirely.

export interface OxyAnswerProps {
  /** Markdown answer text from `useAgentRun().answer`. */
  answer: string | null;
  /** SQL artifacts from `useAgentRun().artifacts`. */
  artifacts?: AgentArtifact[];
  /** Lifecycle state — drives the placeholder, spinner, error UI. */
  state: AgentRunState;
  /** Clarification text when `state === "needs_clarification"`. */
  clarification?: string | null;
  /** Failure reason when `state === "failed"`. */
  error?: Error | null;
  /**
   * @beta Relative URL to the thread view in oxy — renders a
   * "Continue in Oxy (beta)" link when set. Pass `null` to suppress
   * the link entirely; the link is marked beta because the resolved
   * URL may not reach a live thread in every deployment topology
   * (see `UseAgentRunResult.threadUrl`).
   */
  threadUrl?: string | null;
  /** Override the link label. Default: "Continue this thread in Oxy". */
  threadLinkLabel?: string;
  /** Maximum number of SQL result rows to render per artifact. Older
   *  rows truncated with a "+N more" note. Default: 10. */
  maxArtifactRows?: number;
  /** Class on the outer container — for callers using utility CSS. */
  className?: string;
}

/**
 * Renders an agent run's answer + artifacts + thread link as a
 * single block. The default styling is intentionally neutral
 * (system fonts, gray surfaces) so it blends into any bundle.
 *
 * Designed to be paired with `useAgentRun`:
 *
 * ```tsx
 * const run = useAgentRun({ agentId: "analyst" });
 * return (
 *   <>
 *     <button onClick={() => run.ask("how many users last week?")}>Ask</button>
 *     <OxyAnswer {...run} />
 *   </>
 * );
 * ```
 */
export function OxyAnswer(props: OxyAnswerProps): React.JSX.Element {
  const {
    answer,
    artifacts = [],
    state,
    clarification,
    error,
    threadUrl,
    threadLinkLabel = "Continue this thread in Oxy",
    maxArtifactRows = 10,
    className
  } = props;

  const isRunning = state === "running";
  const isFailed = state === "failed";
  const needsClarification = state === "needs_clarification";

  return (
    <div className={className} style={styles.answerWrap}>
      {isRunning && answer === null ? (
        <div style={styles.statusRow}>
          <span style={styles.spinner} aria-hidden='true' />
          <span style={styles.statusText}>Thinking…</span>
        </div>
      ) : null}

      {artifacts.length > 0 ? (
        <div style={styles.artifactList}>
          {artifacts.map((a) => (
            <SqlArtifactBlock key={a.id} artifact={a} maxRows={maxArtifactRows} />
          ))}
        </div>
      ) : null}

      {answer ? (
        <div style={styles.markdown}>
          <MarkdownText text={answer} />
        </div>
      ) : null}

      {needsClarification && clarification ? (
        <div style={styles.clarification}>
          <strong>Agent needs clarification:</strong>
          <div style={{ marginTop: 4 }}>{clarification}</div>
        </div>
      ) : null}

      {isFailed && error ? <ErrorBlock error={error} /> : null}

      {threadUrl && (answer || artifacts.length > 0) ? (
        <div style={styles.threadLinkRow}>
          <a href={threadUrl} target='_blank' rel='noreferrer noopener' style={styles.threadLink}>
            {threadLinkLabel} →
          </a>
          <span style={styles.betaBadge} title='Thread linking is in beta — see docs'>
            beta
          </span>
        </div>
      ) : null}
    </div>
  );
}

export interface OxyChatProps {
  /** Agent id (matches `<id>.agentic.yml` in the project). */
  agentId: string;
  /** Placeholder for the question input. */
  placeholder?: string;
  /** Button label. Default: "Ask". */
  submitLabel?: string;
  /** Rendered when the user hasn't asked anything yet. */
  emptyState?: React.ReactNode;
  /** Forwarded to the inner `<OxyAnswer>`. */
  maxArtifactRows?: number;
  /** Class on the outer container. */
  className?: string;
}

/**
 * Complete drop-in chat surface. One agent, one input, one answer
 * view. The chat is single-turn by default — each new question
 * cancels the previous run and clears the answer. Bundles that
 * want a multi-turn conversation history compose their own UI
 * using `useAgentRun` directly.
 *
 * Single-turn keeps the surface dead simple: bundles use this for
 * the "ask anything about your data" widget that sits next to
 * structured panels. Multi-turn is rare in those contexts and
 * better expressed by the bundle.
 */
export function OxyChat(props: OxyChatProps): React.JSX.Element {
  const {
    agentId,
    placeholder = "Ask a question about your data…",
    submitLabel = "Ask",
    emptyState,
    maxArtifactRows,
    className
  } = props;

  const run = useAgentRun({ agentId });
  const [question, setQuestion] = React.useState("");

  const submit = React.useCallback(
    (e?: React.FormEvent) => {
      e?.preventDefault();
      const q = question.trim();
      if (!q || run.state === "running") return;
      run.ask(q);
    },
    [question, run]
  );

  return (
    <div className={className} style={styles.chatWrap}>
      <form onSubmit={submit} style={styles.chatForm}>
        <input
          type='text'
          value={question}
          onChange={(e) => setQuestion(e.target.value)}
          placeholder={placeholder}
          disabled={run.state === "running"}
          style={styles.chatInput}
          aria-label='Question'
        />
        <button
          type='submit'
          disabled={run.state === "running" || question.trim() === ""}
          style={styles.chatSubmit}
        >
          {run.state === "running" ? "…" : submitLabel}
        </button>
        {run.state === "running" ? (
          <button type='button' onClick={run.cancel} style={styles.chatCancel}>
            Stop
          </button>
        ) : null}
      </form>

      {run.state === "idle" ? (
        (emptyState ?? <div style={styles.emptyState}>Ask a question to get started.</div>)
      ) : (
        <OxyAnswer
          answer={run.answer}
          artifacts={run.artifacts}
          state={run.state}
          clarification={run.clarification}
          error={run.error}
          threadUrl={run.threadUrl}
          maxArtifactRows={maxArtifactRows}
        />
      )}
    </div>
  );
}

function SqlArtifactBlock(props: {
  artifact: AgentSqlArtifact;
  maxRows: number;
}): React.JSX.Element {
  const { artifact, maxRows } = props;
  const [open, setOpen] = React.useState(false);
  const results = artifact.results;
  const truncated = results ? results.rows.length > maxRows : false;
  const visibleRows = results ? results.rows.slice(0, maxRows) : [];
  const sourceLabel =
    artifact.source === "verified_sql"
      ? "Verified query"
      : artifact.source === "semantic_query"
        ? "Semantic query"
        : artifact.source === "omni_query"
          ? "Omni query"
          : "Query";

  return (
    <div style={styles.artifact}>
      <button type='button' onClick={() => setOpen((o) => !o)} style={styles.artifactHeader}>
        <span style={styles.artifactBadge}>{sourceLabel}</span>
        <span style={styles.artifactSummary}>
          {results
            ? `${results.rowCount} row${results.rowCount === 1 ? "" : "s"}`
            : artifact.error
              ? "execution failed"
              : "SQL only"}
        </span>
        <span style={styles.artifactToggle}>{open ? "Hide" : "Show"}</span>
      </button>
      {open ? (
        <div>
          <pre style={styles.sqlBlock}>{artifact.sql}</pre>
          {artifact.error ? (
            <div style={styles.error}>{artifact.error}</div>
          ) : results ? (
            <div style={styles.resultsWrap}>
              <table style={styles.resultsTable}>
                <thead>
                  <tr>
                    {results.columns.map((c) => (
                      <th key={c} style={styles.resultsTh}>
                        {c}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {visibleRows.map((row, i) => (
                    <tr key={i}>
                      {row.map((cell, j) => (
                        <td key={j} style={styles.resultsTd}>
                          {formatCell(cell)}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
              {truncated ? (
                <div style={styles.truncatedNote}>
                  +{results.rows.length - maxRows} more rows. Open the thread in Oxy to see all.
                </div>
              ) : null}
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function formatCell(value: unknown): string {
  if (value === null || value === undefined) return "—";
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

/**
 * Renders a thrown Error with the server's `hint` line broken out
 * if the error is an `OxyApiError`. Falls back to `error.message`
 * for plain Errors. Whitespace-preserving so multi-line hints from
 * the server land readably.
 */
function ErrorBlock(props: { error: Error }): React.JSX.Element {
  const { error } = props;
  if (error instanceof OxyApiError) {
    return (
      <div style={styles.error}>
        <div>
          <strong>Run failed:</strong> {error.message.split("\n\n")[0]}
        </div>
        {error.hint ? (
          <div style={{ marginTop: 6, fontWeight: 400, whiteSpace: "pre-wrap" }}>
            <strong>Hint:</strong> {error.hint}
          </div>
        ) : null}
      </div>
    );
  }
  return (
    <div style={styles.error}>
      <strong>Run failed:</strong> {error.message}
    </div>
  );
}

// ── Minimal markdown renderer ───────────────────────────────────────────────
//
// Agent answers are simple markdown — paragraphs, headings (h1-h3),
// fenced code blocks, inline code, bold, italic, links, unordered
// lists. A 100-line renderer covers it without adding a runtime dep
// (~30 KB) that bundles may already have in a different version.
//
// Out of scope intentionally: tables (use a SQL artifact), images
// (agents don't emit them), nested lists (rare in answers), HTML
// passthrough (no XSS surface).

function MarkdownText(props: { text: string }): React.JSX.Element {
  const blocks = React.useMemo(() => parseMarkdown(props.text), [props.text]);
  return <>{blocks}</>;
}

type MdBlock =
  | { kind: "h"; level: 1 | 2 | 3; text: string }
  | { kind: "p"; text: string }
  | { kind: "code"; lang: string; code: string }
  | { kind: "list"; items: string[] };

function parseMarkdown(text: string): React.JSX.Element[] {
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  const blocks: MdBlock[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (line === undefined) {
      i++;
      continue;
    }
    // Fenced code block
    const fence = line.match(/^```(\w*)\s*$/);
    if (fence) {
      const lang = fence[1] ?? "";
      const buf: string[] = [];
      i++;
      while (i < lines.length && !/^```\s*$/.test(lines[i] ?? "")) {
        buf.push(lines[i] ?? "");
        i++;
      }
      i++; // skip closing fence
      blocks.push({ kind: "code", lang, code: buf.join("\n") });
      continue;
    }
    // Heading
    const h = line.match(/^(#{1,3})\s+(.+)$/);
    if (h) {
      blocks.push({
        kind: "h",
        level: h[1]?.length as 1 | 2 | 3,
        text: h[2]!
      });
      i++;
      continue;
    }
    // List
    if (/^\s*[-*]\s+/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\s*[-*]\s+/.test(lines[i] ?? "")) {
        items.push((lines[i] ?? "").replace(/^\s*[-*]\s+/, ""));
        i++;
      }
      blocks.push({ kind: "list", items });
      continue;
    }
    // Blank line
    if (line.trim() === "") {
      i++;
      continue;
    }
    // Paragraph — collect consecutive non-empty, non-special lines.
    const buf: string[] = [line];
    i++;
    while (i < lines.length) {
      const next = lines[i] ?? "";
      if (
        next.trim() === "" ||
        /^#{1,3}\s+/.test(next) ||
        /^```/.test(next) ||
        /^\s*[-*]\s+/.test(next)
      ) {
        break;
      }
      buf.push(next);
      i++;
    }
    blocks.push({ kind: "p", text: buf.join(" ") });
  }

  return blocks.map((b, idx) => {
    switch (b.kind) {
      case "h": {
        const Tag = `h${b.level}` as unknown as keyof React.JSX.IntrinsicElements;
        const headingStyle = b.level === 1 ? styles.h1 : b.level === 2 ? styles.h2 : styles.h3;
        return (
          <Tag key={idx} style={headingStyle}>
            {renderInline(b.text)}
          </Tag>
        );
      }
      case "code":
        return (
          <pre key={idx} style={styles.codeBlock} data-lang={b.lang || undefined}>
            <code>{b.code}</code>
          </pre>
        );
      case "list":
        return (
          <ul key={idx} style={styles.list}>
            {b.items.map((item, i) => (
              <li key={i}>{renderInline(item)}</li>
            ))}
          </ul>
        );
      case "p":
        return (
          <p key={idx} style={styles.paragraph}>
            {renderInline(b.text)}
          </p>
        );
    }
  });
}

/**
 * Inline tokenizer for **bold**, *italic*, `code`, and [text](url).
 * Lazy: scan once, split into segments. The patterns are matched in
 * priority order (code first so backticks don't get eaten by bold).
 */
function renderInline(text: string): React.ReactNode[] {
  const segments: React.ReactNode[] = [];
  let remaining = text;
  let key = 0;
  // Patterns in priority order — code first since `**` inside backticks
  // shouldn't be parsed.
  const patterns: Array<{
    re: RegExp;
    render: (m: RegExpExecArray) => React.ReactNode;
  }> = [
    { re: /`([^`]+)`/, render: (m) => <code style={styles.inlineCode}>{m[1]}</code> },
    {
      re: /\[([^\]]+)\]\(([^)]+)\)/,
      render: (m) => {
        // Allowlist URL schemes. Agent answers cross the LLM trust
        // boundary — a malicious prompt fragment reflected into the
        // answer could otherwise emit
        // `[click](javascript:alert(document.cookie))` and execute
        // in the bundle's origin. Accept only http(s), mailto,
        // root-relative paths, and same-page fragments; everything
        // else renders as plain text.
        if (isSafeLinkHref(m[2])) {
          return (
            <a href={m[2]} target='_blank' rel='noreferrer noopener' style={styles.link}>
              {m[1]}
            </a>
          );
        }
        return <>{m[1]}</>;
      }
    },
    { re: /\*\*([^*]+)\*\*/, render: (m) => <strong>{m[1]}</strong> },
    { re: /\*([^*]+)\*/, render: (m) => <em>{m[1]}</em> }
  ];

  while (remaining.length > 0) {
    let earliest: { idx: number; len: number; node: React.ReactNode } | null = null;
    for (const { re, render } of patterns) {
      const m = re.exec(remaining);
      if (m && (earliest === null || m.index < earliest.idx)) {
        earliest = { idx: m.index, len: m[0].length, node: render(m) };
      }
    }
    if (earliest === null) {
      segments.push(remaining);
      break;
    }
    if (earliest.idx > 0) segments.push(remaining.slice(0, earliest.idx));
    segments.push(<React.Fragment key={key++}>{earliest.node}</React.Fragment>);
    remaining = remaining.slice(earliest.idx + earliest.len);
  }
  return segments;
}

// ── Styles ──────────────────────────────────────────────────────────────────
//
// All inline. No CSS file, no class names, no global side effects.
// Bundles that want custom styling pass `className` and override
// with their own selectors, or skip the drop-in entirely and build
// on `useAgentRun`.

const SANS =
  '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif';
const MONO = 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace';

const styles: Record<string, React.CSSProperties> = {
  answerWrap: {
    fontFamily: SANS,
    fontSize: 14,
    lineHeight: 1.5,
    color: "#1f2937"
  },
  statusRow: { display: "flex", alignItems: "center", gap: 8, padding: "8px 0" },
  spinner: {
    display: "inline-block",
    width: 12,
    height: 12,
    borderRadius: "50%",
    border: "2px solid #d1d5db",
    borderTopColor: "#6b7280",
    animation: "oxy-spin 0.8s linear infinite"
  },
  statusText: { color: "#6b7280" },
  markdown: { marginTop: 8 },
  h1: { fontSize: 20, fontWeight: 600, margin: "16px 0 8px" },
  h2: { fontSize: 17, fontWeight: 600, margin: "14px 0 6px" },
  h3: { fontSize: 15, fontWeight: 600, margin: "12px 0 4px" },
  paragraph: { margin: "0 0 8px" },
  list: { margin: "0 0 8px", paddingLeft: 20 },
  codeBlock: {
    fontFamily: MONO,
    fontSize: 12,
    background: "#f3f4f6",
    border: "1px solid #e5e7eb",
    borderRadius: 6,
    padding: "8px 10px",
    overflowX: "auto",
    margin: "8px 0"
  },
  inlineCode: {
    fontFamily: MONO,
    fontSize: "0.92em",
    background: "#f3f4f6",
    padding: "1px 4px",
    borderRadius: 3
  },
  link: { color: "#2563eb", textDecoration: "underline" },
  clarification: {
    marginTop: 12,
    padding: "10px 12px",
    background: "#fef3c7",
    border: "1px solid #fcd34d",
    borderRadius: 6,
    color: "#78350f"
  },
  error: {
    marginTop: 12,
    padding: "10px 12px",
    background: "#fee2e2",
    border: "1px solid #fca5a5",
    borderRadius: 6,
    color: "#991b1b"
  },
  threadLinkRow: {
    marginTop: 12,
    textAlign: "right",
    display: "flex",
    justifyContent: "flex-end",
    alignItems: "center",
    gap: 6
  },
  threadLink: { fontSize: 12, color: "#6b7280", textDecoration: "none" },
  betaBadge: {
    fontSize: 9,
    fontWeight: 600,
    letterSpacing: 0.5,
    textTransform: "uppercase",
    padding: "1px 5px",
    borderRadius: 3,
    background: "#fef3c7",
    color: "#92400e",
    border: "1px solid #fcd34d"
  },
  artifactList: { display: "flex", flexDirection: "column", gap: 8, marginBottom: 8 },
  artifact: {
    border: "1px solid #e5e7eb",
    borderRadius: 6,
    background: "#fafafa",
    overflow: "hidden"
  },
  artifactHeader: {
    display: "flex",
    alignItems: "center",
    gap: 10,
    width: "100%",
    padding: "6px 10px",
    background: "transparent",
    border: "none",
    borderBottom: "1px solid transparent",
    cursor: "pointer",
    fontFamily: SANS,
    fontSize: 12,
    color: "#374151"
  },
  artifactBadge: {
    fontWeight: 600,
    fontSize: 11,
    textTransform: "uppercase",
    letterSpacing: 0.4,
    color: "#4b5563"
  },
  artifactSummary: { color: "#6b7280", flex: 1 },
  artifactToggle: { color: "#2563eb" },
  sqlBlock: {
    fontFamily: MONO,
    fontSize: 12,
    margin: 0,
    padding: "8px 10px",
    background: "#0f172a",
    color: "#e2e8f0",
    overflowX: "auto"
  },
  resultsWrap: { padding: 8, overflowX: "auto" },
  resultsTable: { width: "100%", borderCollapse: "collapse", fontSize: 12 },
  resultsTh: {
    textAlign: "left",
    padding: "4px 8px",
    borderBottom: "1px solid #e5e7eb",
    fontWeight: 600,
    color: "#374151"
  },
  resultsTd: { padding: "4px 8px", borderBottom: "1px solid #f3f4f6", color: "#1f2937" },
  truncatedNote: { fontSize: 11, color: "#6b7280", padding: "6px 8px" },
  chatWrap: { fontFamily: SANS, fontSize: 14, color: "#1f2937" },
  chatForm: { display: "flex", gap: 8, marginBottom: 12 },
  chatInput: {
    flex: 1,
    padding: "8px 12px",
    border: "1px solid #d1d5db",
    borderRadius: 6,
    fontSize: 14,
    fontFamily: SANS
  },
  chatSubmit: {
    padding: "8px 16px",
    border: "none",
    borderRadius: 6,
    background: "#2563eb",
    color: "#ffffff",
    fontSize: 14,
    fontWeight: 500,
    cursor: "pointer"
  },
  chatCancel: {
    padding: "8px 12px",
    border: "1px solid #d1d5db",
    borderRadius: 6,
    background: "#ffffff",
    color: "#374151",
    fontSize: 14,
    cursor: "pointer"
  },
  emptyState: { padding: "12px 0", color: "#9ca3af", fontStyle: "italic" }
};

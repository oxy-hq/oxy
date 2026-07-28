// React hooks for the metric-tree analysis ops, exposed to custom-app
// bundles so an app can run drivers / what-if / RCA / opportunity sizing
// without hand-rolling fetch against the semantic layer.
//
// These wrap the `/api/projects/{id}/semantic/metric-tree*` endpoints —
// the same airlayer analyses the IDE's World Model and Metric Tree
// surfaces drive — behind the shared `OxyAppProvider` fetcher (session
// cookie in-workspace, dev-proxy token cross-origin). Response shapes
// reuse the wire types in `../metricTree`, so a bundle typed against a
// hook result matches what the server serializes verbatim.
//
// Pattern mirrors `useSemanticQuery`: each hook fetches when enabled and
// its input is present, re-runs on input change (deep-compared via JSON),
// and exposes `refetch`. Read-only inputs (`null`) keep a hook idle — the
// natural fit for "run once the user picks a target measure".

import * as React from "react";
import type {
  DistributionRequest,
  ExplainRequest,
  ExplainResult,
  MetricTree,
  OpportunityRequest,
  OpportunityResult,
  PredictChange,
  PredictResult,
  SensitivityResult,
  TimeDimensionsResponse
} from "../metricTree";
import { getJson, metricTreePath, postJson } from "./metric-tree-fetch";
import { useOxyApp } from "./react";

/** Shared result envelope for every metric-tree hook. */
export interface MetricTreeHookResult<Data> {
  data: Data | null;
  loading: boolean;
  error: Error | null;
  /** Force a re-run, bypassing nothing — the server honors `?refresh`. */
  refetch: () => void;
}

interface EndpointOpts {
  /** Set false to skip the request (e.g. waiting on a user selection). */
  enabled?: boolean;
}

/**
 * Internal engine shared by every metric-tree hook. Runs `run(signal)`
 * whenever `key` changes (and on `refetch`), tracks loading/error, and
 * cancels in-flight work on unmount or input change.
 *
 * `key` is the deep-compare fingerprint of the request; a `null` key
 * means "no request yet" and leaves the hook idle without firing.
 */
function useMetricTreeEndpoint<Data>(
  key: string | null,
  run: (signal: AbortSignal) => Promise<Data>,
  enabled: boolean
): MetricTreeHookResult<Data> {
  const [data, setData] = React.useState<Data | null>(null);
  const [loading, setLoading] = React.useState<boolean>(enabled && key !== null);
  const [error, setError] = React.useState<Error | null>(null);
  const [_nonce, setNonce] = React.useState(0);

  // `run` is re-created each render; pin the latest in a ref so the effect
  // depends only on `key`/`enabled`/`nonce` and doesn't re-fire on every
  // parent render.
  const runRef = React.useRef(run);
  runRef.current = run;

  React.useEffect(() => {
    if (!enabled || key === null) {
      setLoading(false);
      return;
    }
    const ctrl = new AbortController();
    let cancelled = false;
    setLoading(true);
    setError(null);

    runRef
      .current(ctrl.signal)
      .then((result) => {
        if (cancelled) return;
        setData(result);
        setLoading(false);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        if (err instanceof DOMException && err.name === "AbortError") return;
        setError(err instanceof Error ? err : new Error(String(err)));
        setLoading(false);
      });

    return () => {
      cancelled = true;
      ctrl.abort();
    };
  }, [key, enabled]);

  const refetch = React.useCallback(() => setNonce((n) => n + 1), []);
  return { data, loading, error, refetch };
}

// ── useMetricTree ─────────────────────────────────────────────────────────────

export interface UseMetricTreeOpts extends EndpointOpts {
  /** Optional measure id to root the returned subtree at. */
  root?: string;
}

/**
 * The project's metric tree — measures (nodes) and their component /
 * driver relationships (edges) — or the subtree rooted at `opts.root`.
 * The structural backbone every other metric-tree analysis reads against.
 */
export function useMetricTree(opts: UseMetricTreeOpts = {}): MetricTreeHookResult<MetricTree> {
  const { projectId, fetcher } = useOxyApp();
  const enabled = opts.enabled !== false;
  const root = opts.root;
  const key = projectId ? JSON.stringify({ projectId, root }) : null;

  return useMetricTreeEndpoint<MetricTree>(
    key,
    (signal) => {
      const qs = root ? `?root=${encodeURIComponent(root)}` : "";
      return getJson<MetricTree>(fetcher, `${metricTreePath(projectId as string)}${qs}`, signal);
    },
    enabled
  );
}

// ── useSensitivity (drivers) ──────────────────────────────────────────────────

/**
 * Ranked drivers of `measureId`, by influence — the "what moves this
 * measure" question. Pass `null` to stay idle until a measure is chosen.
 */
export function useSensitivity(
  measureId: string | null,
  opts: EndpointOpts = {}
): MetricTreeHookResult<SensitivityResult> {
  const { projectId, fetcher } = useOxyApp();
  const enabled = opts.enabled !== false;
  const key = projectId && measureId ? JSON.stringify({ projectId, measureId }) : null;

  return useMetricTreeEndpoint<SensitivityResult>(
    key,
    (signal) => {
      const path = `${metricTreePath(projectId as string)}/${encodeURIComponent(
        measureId as string
      )}/sensitivity`;
      return getJson<SensitivityResult>(fetcher, path, signal);
    },
    enabled
  );
}

// ── usePredict (what-if) ──────────────────────────────────────────────────────

/**
 * Propagate hypothetical `(measure, delta)` changes upward through the
 * tree and return the estimated impact on every downstream measure — a
 * pure metric-tree walk, no warehouse query. Pass `null` to stay idle.
 */
export function usePredict(
  changes: PredictChange[] | null,
  opts: EndpointOpts = {}
): MetricTreeHookResult<PredictResult> {
  const { projectId, fetcher } = useOxyApp();
  const enabled = opts.enabled !== false;
  const key = projectId && changes ? JSON.stringify({ projectId, changes }) : null;

  return useMetricTreeEndpoint<PredictResult>(
    key,
    (signal) =>
      postJson<PredictResult>(
        fetcher,
        `${metricTreePath(projectId as string)}/predict`,
        { changes },
        signal
      ),
    enabled
  );
}

// ── useExplain (RCA) ──────────────────────────────────────────────────────────

/**
 * Period-over-period root-cause decomposition: recursively splits the
 * target measure by components and dimensions until the move concentrates.
 * This is the heavy one — it can fire many warehouse queries and the
 * server caps it at 45s. Pass `null` to defer until periods are chosen.
 */
export function useExplain(
  request: ExplainRequest | null,
  opts: EndpointOpts = {}
): MetricTreeHookResult<ExplainResult> {
  const { projectId, fetcher } = useOxyApp();
  const enabled = opts.enabled !== false;
  const key = projectId && request ? JSON.stringify({ projectId, request }) : null;

  return useMetricTreeEndpoint<ExplainResult>(
    key,
    (signal) =>
      postJson<ExplainResult>(
        fetcher,
        `${metricTreePath(projectId as string)}/explain`,
        request,
        signal
      ),
    enabled
  );
}

// ── useDistribution ───────────────────────────────────────────────────────────

/**
 * Single-period distribution of a measure — an {@link ExplainResult}
 * against an auto-derived immediately-prior baseline. Same renderers as
 * `useExplain`; ignore the delta fields for a pure distribution view.
 */
export function useDistribution(
  request: DistributionRequest | null,
  opts: EndpointOpts = {}
): MetricTreeHookResult<ExplainResult> {
  const { projectId, fetcher } = useOxyApp();
  const enabled = opts.enabled !== false;
  const key = projectId && request ? JSON.stringify({ projectId, request }) : null;

  return useMetricTreeEndpoint<ExplainResult>(
    key,
    (signal) =>
      postJson<ExplainResult>(
        fetcher,
        `${metricTreePath(projectId as string)}/distribution`,
        request,
        signal
      ),
    enabled
  );
}

// ── useOpportunity (sizing) ───────────────────────────────────────────────────

/**
 * Segment opportunity sizing for a measure over a period: finds
 * underperforming segments and sizes the addressable upside of closing
 * each rate gap against a benchmark peer. Pass `null` to stay idle until
 * a target + period are chosen.
 */
export function useOpportunity(
  request: OpportunityRequest | null,
  opts: EndpointOpts = {}
): MetricTreeHookResult<OpportunityResult> {
  const { projectId, fetcher } = useOxyApp();
  const enabled = opts.enabled !== false;
  const key = projectId && request ? JSON.stringify({ projectId, request }) : null;

  return useMetricTreeEndpoint<OpportunityResult>(
    key,
    (signal) =>
      postJson<OpportunityResult>(
        fetcher,
        `${metricTreePath(projectId as string)}/opportunity`,
        request,
        signal
      ),
    enabled
  );
}

// ── useTimeDimensions ─────────────────────────────────────────────────────────

/**
 * The queryable time dimensions per view (`view.dim` ids) — what a
 * bundle offers as the period axis for `explain` / `opportunity` /
 * `distribution` instead of hardcoding a curated map.
 */
export function useTimeDimensions(
  opts: EndpointOpts = {}
): MetricTreeHookResult<TimeDimensionsResponse> {
  const { projectId, fetcher } = useOxyApp();
  const enabled = opts.enabled !== false;
  const key = projectId ? JSON.stringify({ projectId, kind: "time-dimensions" }) : null;

  return useMetricTreeEndpoint<TimeDimensionsResponse>(
    key,
    (signal) =>
      getJson<TimeDimensionsResponse>(
        fetcher,
        `${metricTreePath(projectId as string)}/time-dimensions`,
        signal
      ),
    enabled
  );
}

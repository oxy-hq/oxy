// Anomaly inbox types + client. Surfaces the `/semantic/anomalies*`
// endpoints — list, scan, status, explain — so SDK consumers can render
// the same inbox the Oxy IDE uses.

import type { OxyConfig } from "./config";
import type { ExplainResult } from "./metricTree";

// ── Types ────────────────────────────────────────────────────────────────────

export type AnomalyStatus = "new" | "acknowledged" | "dismissed";
export type AnomalySeverity = "low" | "medium" | "high";

/** One filter pinning an anomaly (or a failed monitor) to a segment. */
export interface AnomalyFilter {
  /** Fully-qualified dimension id, e.g. `"sales_daily.restaurant_id"`. */
  member: string;
  /** Matched values (OR within a filter). */
  values: string[];
}

/**
 * One row in the anomaly inbox. Detected by `oxy-metric-monitoring` per
 * `.monitor.yml` entry; upserted by repeat scans so unresolved anomalies
 * stay visible without piling up duplicates.
 */
export interface Anomaly {
  id: string;
  workspace_id: string;
  measure: string;
  time_dimension: string;
  granularity: string;
  period_start: string;
  period_end: string;
  observed: number;
  expected: number;
  lower_bound: number;
  upper_bound: number;
  z_score: number;
  severity: AnomalySeverity | string;
  status: AnomalyStatus | string;
  label?: string | null;
  /**
   * Stable key derived from the monitor's filters (e.g.
   * `"sales_daily.restaurant_id=loc-abc"`). Empty for chain-wide monitors.
   */
  dimension_key: string;
  /**
   * Raw filters identifying the segment; `null` for chain-wide monitors.
   * Always present on the wire (the server serializes it unconditionally),
   * hence required-nullable rather than optional — same shape as
   * {@link ScanFailure.filters}.
   */
  filters: AnomalyFilter[] | null;
  /**
   * Groups consecutive flagged buckets of one segment into a single event, so a
   * surge spanning Mon/Wed/Thu reads as one problem rather than three. `null`
   * for rows detected before events existed. This is what
   * {@link AnomaliesClient.updateStatusBulk} wants as `eventIds` — a status
   * action applies to the whole event.
   */
  event_id?: string | null;
  /** Cached ExplainResult — populated by `POST /anomalies/:id/explain`. */
  explain_cache?: ExplainResult | null;
  explain_cached_at?: string | null;
  detected_at: string;
  updated_at: string;
}

export interface ListAnomaliesOptions {
  status?: AnomalyStatus | string;
  /**
   * Max **events** (server caps at 500, defaults to 100). Every bucket of a
   * returned event comes back, so the row count is `limit × buckets-per-event`.
   * With `order: "recent"` it is a plain row limit instead.
   */
  limit?: number;
  /**
   * How many **events** to skip (rows, with `order: "recent"`) — same unit as
   * `limit`, so page `n` is `offset: (n - 1) * limit`. Defaults to 0.
   *
   * Bounded: past the server's maximum depth the request is refused with a 400
   * rather than served a repeat of the last reachable page, so a runaway
   * `offset += limit` loop ends loudly instead of spinning. Every response
   * echoes that depth as `max_offset`, so a loop can stop before reaching it.
   */
  offset?: number;
  /**
   * `"recent"` returns latest-first (`detected_at DESC`). Omit for the default
   * worst-first ranking by event severity (active events before dismissed).
   */
  order?: "recent";
}

export interface ListAnomaliesResponse {
  anomalies: Anomaly[];
  /**
   * Total matching the filter across every page — **events** under the default
   * ranking, rows under `order: "recent"`. Same unit as `limit`/`offset`, so
   * `Math.ceil(total / limit)` is the page count. Note it will not equal
   * `anomalies.length` under the default ranking even on a single page: each
   * event returns all of its buckets.
   *
   * **Absent** in two cases, and a client that pages has to handle both. Send
   * neither `limit` nor `offset` and you have asked for "the top N", so there
   * is no total behind the answer — the field is omitted rather than filled
   * with the page's own length. Pass a `limit` (with `offset: 0` for the first
   * page) to get a real total to loop against.
   *
   * It is also dropped when the count query itself fails: the page rows are
   * already in hand, and the server serves them without their denominator
   * rather than failing a request it could answer. So a page you asked for
   * with `limit` can still come back untotalled — page off `anomalies.length`
   * and `max_offset` in that case rather than treating it as zero.
   */
  total?: number;
  /**
   * The page actually served. `limit` is clamped to 1..=500, so it can come
   * back smaller than you asked for and every page number you compute must
   * divide by this rather than by what you sent. `offset` is *not* clamped —
   * too deep a request is refused with a 400 (see `max_offset`), so this echoes
   * the offset you sent whenever there is a response at all.
   *
   * Optional because a replica still running a pre-paging build emits neither,
   * which is a live shape during a rolling deploy. Fall back to what you asked
   * for rather than doing arithmetic on `undefined`.
   */
  limit?: number;
  offset?: number;
  /**
   * The deepest `offset` the server will serve — past it a request is refused
   * with a 400. Read it rather than hardcoding a copy: a paging loop bounded by
   * this stops cleanly instead of ending on an error.
   */
  max_offset?: number;
  /**
   * Event keys whose buckets were trimmed to the server's per-event cap (50) —
   * an `event_id`, or `ungrouped:<row id>` for a row detected before events
   * existed. For those events `anomalies` holds the worst buckets, not all of
   * them, so a status write should name the event through `updateStatusBulk`'s
   * `eventIds` rather than enumerating the buckets you received.
   *
   * Only meaningful under the default ranking, which pages *events* and
   * returns each whole — there, an absence means complete. With
   * `order: "recent"` the page is row-limited, so an event can straddle its
   * boundary instead; this list stays empty and every event should be treated
   * as possibly partial.
   */
  truncated_events?: string[];
}

export interface BulkUpdateStatusResponse {
  /** Buckets actually written. Lower than what you sent when a row was
   *  deleted, moved out of `onlyStatus`, or belongs to another workspace. */
  updated: number;
  /** Distinct anomalies behind those buckets — events, plus standalone
   *  pre-event rows. The unit a UI counts in, and one only the server can
   *  compute: naming an event never told you how many buckets it held.
   *
   *  An anomaly counts as updated once *any* of its buckets is written. Name
   *  events through `eventIds` and that is the whole anomaly; name one bucket
   *  of a long chain through `ids` and this still reports `1` while the rest
   *  keep their old status. `ids` is for pre-event rows, which hold one bucket
   *  each — using it for anything else buys a partial write. */
  events_updated: number;
}

export interface ScanOptions {
  /** Override the reference "now" date (YYYY-MM-DD) — useful for demos. */
  as_of?: string;
}

/** One `.monitor.yml` entry that errored during a scan. */
export interface ScanFailure {
  measure: string;
  time_dimension: string;
  granularity: string;
  label: string | null;
  /** Segment key for a `group_by`/filtered monitor; empty for chain-wide. */
  dimension_key: string;
  /** Raw filters identifying the segment; null for chain-wide monitors. */
  filters: AnomalyFilter[] | null;
  error: string;
}

export interface ScanResponse {
  monitors_scanned: number;
  monitors_failed: number;
  anomalies_persisted: number;
  /**
   * True when the scan is still running server-side (it exceeded the 55 s
   * synchronous window, or a scan started within the last 60 s and this call
   * was debounced). The counts are all `0` in that case — they are NOT a
   * "nothing found" result. Refetch with `list()` after a short delay.
   */
  pending: boolean;
  /**
   * Per-monitor failures. Empty array (never absent) on a clean scan and on
   * the `pending` path, where failures aren't known yet.
   */
  failures: ScanFailure[];
}

export interface ExplainOptions {
  /** Recompute even when the row already has a cached result. */
  refresh?: boolean;
}

// ── Client ───────────────────────────────────────────────────────────────────

export type RequestFn = <T>(endpoint: string, options?: RequestInit) => Promise<T>;

/** Which buckets a write may touch when the caller didn't say. Live statuses
 *  for ack/dismiss; all three for a reopen, which exists to reach dismissed
 *  ones. */
function defaultScope(status: AnomalyStatus): AnomalyStatus[] {
  return status === "new" ? ["new", "acknowledged", "dismissed"] : ["new", "acknowledged"];
}

/**
 * Client for `/semantic/anomalies*`. Construct via `OxyClient.anomalies`
 * rather than instantiating directly — the getter wires the request helper
 * so auth, timeout, and branch propagation come along for free.
 *
 * @example
 * ```typescript
 * const { anomalies } = await client.anomalies.list({ status: "new" });
 * for (const a of anomalies) {
 *   console.log(a.label ?? a.measure, a.severity, a.z_score.toFixed(2));
 * }
 * ```
 */
export class AnomaliesClient {
  private readonly request: RequestFn;
  private readonly config: OxyConfig;

  constructor(config: OxyConfig, request: RequestFn) {
    this.config = config;
    this.request = request;
  }

  private path(suffix: string): string {
    return `/${this.config.projectId}/semantic/anomalies${suffix}`;
  }

  private buildQuery(extra: Record<string, string> = {}): string {
    const params: Record<string, string> = { ...extra };
    if (this.config.branch) params.branch = this.config.branch;
    const qs = new URLSearchParams(params).toString();
    return qs ? `?${qs}` : "";
  }

  /**
   * List anomalies in the inbox, ranked worst-first by event severity (active
   * events before dismissed). Pass `order: "recent"` for latest-first.
   *
   * @example
   * ```typescript
   * // Open / unresolved anomalies only
   * const { anomalies } = await client.anomalies.list({ status: "new" });
   *
   * // Second page of 25 events
   * const page2 = await client.anomalies.list({ limit: 25, offset: 25 });
   * console.log(`${(page2.offset ?? 25) + 1}+ of ${page2.total ?? "?"}`);
   * ```
   */
  async list(options: ListAnomaliesOptions = {}): Promise<ListAnomaliesResponse> {
    const extra: Record<string, string> = {};
    if (options.status) extra.status = options.status;
    // Presence, not truthiness. The server reads "is this caller paging?" off
    // whether these params were sent at all, so dropping `offset: 0` on a
    // falsy check would make the first iteration of a paging loop a non-paging
    // request — one that reports `total` as just the rows it returned, ending
    // the loop after a single page.
    if (options.limit !== undefined) extra.limit = String(options.limit);
    if (options.offset !== undefined) extra.offset = String(options.offset);
    if (options.order) extra.order = options.order;
    // No trailing slash before the query — axum 307-redirects "/anomalies/"
    // to "/anomalies", and the redirect fails CORS preflight in browsers.
    return this.request<ListAnomaliesResponse>(this.path(this.buildQuery(extra)));
  }

  /**
   * Trigger a full scan. Iterates every `.monitor.yml` entry in the
   * workspace, runs the detector, and upserts matching rows into the
   * inbox. Returns counts of scanned / failed / persisted.
   *
   * Long-running: the server waits up to 55 s, then returns
   * `pending: true` with zeroed counts while the scan finishes in the
   * background. Always check `pending` before treating `0` as "nothing
   * found", and refetch with {@link list} shortly after.
   *
   * @example
   * ```typescript
   * // Scan against a known-good reference date (matches the seed dataset)
   * const result = await client.anomalies.scan({ as_of: "2025-12-15" });
   * if (result.pending) {
   *   console.log("scan still running — refetch shortly");
   * } else {
   *   console.log(`${result.anomalies_persisted} anomalies detected`);
   * }
   * ```
   */
  async scan(options: ScanOptions = {}): Promise<ScanResponse> {
    const extra: Record<string, string> = {};
    if (options.as_of) extra.as_of = options.as_of;
    return this.request<ScanResponse>(this.path(`/scan${this.buildQuery(extra)}`), {
      method: "POST"
    });
  }

  /**
   * Update an anomaly's status (acknowledge / dismiss / re-open).
   */
  async updateStatus(anomalyId: string, status: AnomalyStatus): Promise<Anomaly> {
    const query = this.buildQuery();
    return this.request<Anomaly>(this.path(`/${encodeURIComponent(anomalyId)}/status${query}`), {
      method: "POST",
      body: JSON.stringify({ status })
    });
  }

  /**
   * Update many anomalies in one request — the batch form of
   * {@link updateStatus}. Identifiers outside the workspace are skipped rather
   * than erroring, so `updated` (rows written) can be lower than what you sent.
   * At most 2000 identifiers across both lists.
   *
   * **Prefer `eventIds`.** Inbox actions are per *event*, and a list response
   * caps how many buckets it returns per event — so acking the bucket ids you
   * received can leave the tail of a long chain behind, `new`, under a clean
   * success. Naming the event lets the server write all of it. `ids` is for
   * rows with no `event_id` (detected before events existed), which can only
   * be named individually.
   *
   * `onlyStatuses` says which of an event's buckets may move. An event can span
   * statuses, so an unbounded write resurrects buckets that were dismissed on
   * purpose — which is why omitting it takes a scope rather than no bound at
   * all: the live statuses (`["new", "acknowledged"]`) for an ack or dismiss,
   * and all three for `status: "new"`, since reopening is how a dismissed
   * anomaly comes back. The server applies that same default, so the safe
   * behaviour does not depend on going through this client. Pass `[]` to opt
   * out of the bound entirely.
   *
   * @example
   * ```typescript
   * const { anomalies } = await client.anomalies.list({ status: "new", limit: 50, offset: 0 });
   * // Both lists: events by id, and pre-event rows (no `event_id`) by their own.
   * const eventIds = [...new Set(anomalies.flatMap((a) => (a.event_id ? [a.event_id] : [])))];
   * const ids = anomalies.filter((a) => !a.event_id).map((a) => a.id);
   * const { updated } = await client.anomalies.updateStatusBulk(
   *   { ids, eventIds, onlyStatuses: ["new", "acknowledged"] },
   *   "acknowledged"
   * );
   * ```
   */
  async updateStatusBulk(
    target: { ids?: string[]; eventIds?: string[]; onlyStatuses?: AnomalyStatus[] },
    status: AnomalyStatus
  ): Promise<BulkUpdateStatusResponse> {
    return this.request<BulkUpdateStatusResponse>(this.path(`/status${this.buildQuery()}`), {
      method: "POST",
      body: JSON.stringify({
        ids: target.ids ?? [],
        event_ids: target.eventIds ?? [],
        // Defaults to a scope, never to "no bound": an empty list tells the
        // server to write every bucket of the named events, dismissed ones
        // included, and that is the single state this design says must not be
        // reversed by accident.
        //
        // Reopening is the exception. `status: "new"` is how a dismissed
        // anomaly comes back, so excluding `dismissed` there would make the
        // one call that needs it a silent no-op.
        only_statuses: target.onlyStatuses ?? defaultScope(status),
        status
      })
    });
  }

  /**
   * Run the metric-tree `explain` for an anomaly and cache the result on
   * the row. Subsequent calls return the cached `ExplainResult` instantly;
   * pass `{ refresh: true }` to bust the cache and recompute.
   *
   * The uncached path runs a 20-30 s recursive driver search — budget for it
   * (or read `explain_cache` off the row from {@link list} when it's already
   * populated).
   */
  async explain(anomalyId: string, options: ExplainOptions = {}): Promise<ExplainResult> {
    const extra: Record<string, string> = {};
    if (options.refresh) extra.refresh = "true";
    return this.request<ExplainResult>(
      this.path(`/${encodeURIComponent(anomalyId)}/explain${this.buildQuery(extra)}`),
      { method: "POST" }
    );
  }
}

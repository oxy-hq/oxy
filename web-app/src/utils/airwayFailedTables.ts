/**
 * The one derivation of "which tables failed in this airway run" — the
 * list the "Re-run failed tables" offer is built from, shared by the IDE
 * pipeline page (`airwayReducer`) and the coordinator run detail
 * (`Elt/model.ts`) so the two surfaces can never disagree.
 *
 * Two rules, both load-bearing:
 *
 * 1. **Only a per-table failure event marks a table failed** —
 *    `resource_failed` (extract skipped) or `table_load_failed` (load
 *    skipped). The RUN status is never consulted: a run can be `failed`
 *    at the run level while every table loaded (e.g. a server restart
 *    after the load finished), and offering to re-run tables that never
 *    failed is exactly the phantom this module exists to prevent.
 * 2. **Only the latest attempt counts.** A reset-in-place re-drive keeps
 *    the SAME run id, so a prior attempt's events stay on the run (the
 *    backend's `classify_run_outcome` reads `since_seq` for the same
 *    reason). Each `load_started` opens a new attempt and clears what the
 *    previous one recorded. Within an attempt a failure is sticky, the
 *    same as the backend's `classify_run_outcome`: a skipped table does not
 *    emit `table_loaded`, so there is nothing that could supersede it.
 */

export type FailedTable = { table: string; error: string };

const FAILURE_EVENTS = new Set(["resource_failed", "table_load_failed"]);

const tableOf = (payload: unknown): string => {
  const t = (payload as { table?: unknown } | null | undefined)?.table;
  return typeof t === "string" ? t : "";
};

const errorOf = (payload: unknown, fallback: string): string => {
  const e = (payload as { error?: unknown } | null | undefined)?.error;
  return typeof e === "string" && e.length > 0 ? e : fallback;
};

/**
 * Incremental form, for folds that need the list mid-stream (the IDE
 * reducer decides `completed_with_errors` at `load_completed`). Returns
 * `true` when the event changed the failed set.
 */
export class FailedTableTracker {
  private failed = new Map<string, string>();

  observe(eventType: string, payload: unknown): boolean {
    if (eventType === "load_started") {
      if (this.failed.size === 0) return false;
      this.failed.clear();
      return true;
    }
    if (FAILURE_EVENTS.has(eventType)) {
      const table = tableOf(payload);
      // A failure with no table can't be retried per-table; the run-level
      // error surfaces it instead.
      if (!table || this.failed.has(table)) return false;
      // First error wins: the streaming path can emit both a
      // `resource_failed` and a `table_load_failed` for one table.
      this.failed.set(
        table,
        errorOf(payload, eventType === "resource_failed" ? "extract failed" : "load failed")
      );
      return true;
    }
    return false;
  }

  errorFor(table: string): string | undefined {
    return this.failed.get(table);
  }

  /** Failed tables of the latest attempt, in first-failure order. */
  list(): FailedTable[] {
    return Array.from(this.failed, ([table, error]) => ({ table, error }));
  }
}

/** Fold a whole event list. `type` is the event name (`event_type` on
 *  coordinator rows, `type` on the SSE stream). */
export function deriveFailedTables(
  events: Iterable<{ type: string; payload: unknown }>
): FailedTable[] {
  const tracker = new FailedTableTracker();
  for (const e of events) tracker.observe(e.type, e.payload);
  return tracker.list();
}

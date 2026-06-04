import { useMemo } from "react";
import type { QueueRow } from "@/services/api/internalJobs";

export interface FailureGroup {
  key: string;
  /** Short label rendered as the group header. */
  label: string;
  rows: QueueRow[];
}

/**
 * Group dead-letter / failed rows by their failure shape so the
 * operator can collapse N copies of the same bug into one row.
 *
 * Today's `QueueRow` doesn't carry the error text — the backend can
 * surface that later. Until then we group by `task_type` as a proxy
 * (most common pivot: "97 dead AnalyticsRun" vs "12 dead PreaggRun").
 * The grouping primitive is the contract; swapping the key for a
 * normalized error prefix is a one-line change once the field lands.
 */
export function useFailureShapes(rows: QueueRow[]): FailureGroup[] {
  return useMemo(() => {
    const byKey = new Map<string, QueueRow[]>();
    for (const row of rows) {
      const key = normalizeKey(row);
      const bucket = byKey.get(key);
      if (bucket) bucket.push(row);
      else byKey.set(key, [row]);
    }
    return [...byKey.entries()]
      .map(([key, groupRows]) => ({
        key,
        label: labelFor(key, groupRows),
        rows: groupRows
      }))
      .sort((a, b) => b.rows.length - a.rows.length);
  }, [rows]);
}

function normalizeKey(row: QueueRow): string {
  return row.task_type ?? "(unknown)";
}

function labelFor(key: string, rows: QueueRow[]): string {
  return `${key} · ${rows.length}`;
}

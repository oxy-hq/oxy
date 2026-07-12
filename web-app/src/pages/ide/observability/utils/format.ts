import { formatDistanceToNow } from "date-fns";

export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "—";
  if (ms < 1) return `${(ms * 1000).toFixed(0)}µs`;
  if (ms < 1000) return `${ms.toFixed(0)}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(2)}s`;
  return `${(ms / 60000).toFixed(1)}m`;
}

/**
 * Relative "time ago" for a backend timestamp, hardened against bad input.
 *
 * date-fns `formatDistanceToNow` throws `RangeError: Invalid time value` on an
 * empty or unparseable date. An unguarded throw here render-crashes the entire
 * observability surface (the trace-detail "Invalid time value" incident), so a
 * missing/invalid timestamp degrades to an em dash instead of taking the page
 * down. All observability "time ago" rendering routes through this helper.
 */
export function formatTimeAgo(timestamp: string | null | undefined): string {
  if (!timestamp) return "—";
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) return "—";
  return formatDistanceToNow(date, { addSuffix: true });
}

import { getDurationMs, getTokensTotal, type Trace } from "@/services/api/traces";
import type { DurationBucket, TimeBucket, TraceStats } from "./types";

/**
 * Format duration in milliseconds to human readable string
 */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms.toFixed(0)}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(2)}s`;
  return `${(ms / 60000).toFixed(2)}m`;
}

/**
 * Aggregate traces into time buckets for time-series charts
 */
export function aggregateByTime(traces: Trace[]): TimeBucket[] {
  if (!traces || traces.length === 0) return [];

  const sorted = [...traces].sort(
    (a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime()
  );

  const firstTime = new Date(sorted[0].timestamp).getTime();
  const lastTime = new Date(sorted[sorted.length - 1].timestamp).getTime();
  const timeRange = lastTime - firstTime;

  // Choose bucket size based on time range
  let bucketMs: number;
  let formatFn: (date: Date) => string;

  if (timeRange < 3600000) {
    // Less than 1 hour -> 5 minute buckets
    bucketMs = 300000;
    formatFn = (d) => d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  } else if (timeRange < 86400000) {
    // Less than 1 day -> 1 hour buckets
    bucketMs = 3600000;
    formatFn = (d) => d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  } else if (timeRange < 604800000) {
    // Less than 1 week -> 6 hour buckets
    bucketMs = 21600000;
    formatFn = (d) =>
      d.toLocaleDateString([], { month: "short", day: "numeric" }) +
      " " +
      d.toLocaleTimeString([], { hour: "2-digit" });
  } else {
    // 1 day buckets
    bucketMs = 86400000;
    formatFn = (d) => d.toLocaleDateString([], { month: "short", day: "numeric" });
  }

  const buckets = new Map<number, TimeBucket>();

  for (const trace of sorted) {
    const time = new Date(trace.timestamp).getTime();
    const bucketKey = Math.floor(time / bucketMs) * bucketMs;

    if (!buckets.has(bucketKey)) {
      buckets.set(bucketKey, {
        time: formatFn(new Date(bucketKey)),
        agentCount: 0,
        workflowCount: 0,
        tokens: 0
      });
    }

    const bucket = buckets.get(bucketKey)!;
    if (trace.spanName === "agent.run_agent") {
      bucket.agentCount++;
    } else if (trace.spanName === "workflow.run_workflow") {
      bucket.workflowCount++;
    }
    bucket.tokens += getTokensTotal(trace) ?? 0;
  }

  return Array.from(buckets.values());
}

/**
 * Create duration distribution buckets for histogram
 */
export function aggregateByDuration(traces: Trace[]): DurationBucket[] {
  if (!traces || traces.length === 0) return [];

  const durations = traces.map((t) => getDurationMs(t));
  const maxDuration = Math.max(...durations);

  // Create logarithmic-ish buckets for better visualization
  const bucketRanges: [number, number, string][] = [];

  if (maxDuration <= 1000) {
    bucketRanges.push(
      [0, 100, "0-100ms"],
      [100, 200, "100-200ms"],
      [200, 500, "200-500ms"],
      [500, 1000, "500ms-1s"]
    );
  } else if (maxDuration <= 10000) {
    bucketRanges.push(
      [0, 500, "0-500ms"],
      [500, 1000, "500ms-1s"],
      [1000, 2000, "1-2s"],
      [2000, 5000, "2-5s"],
      [5000, 10000, "5-10s"]
    );
  } else {
    bucketRanges.push(
      [0, 1000, "0-1s"],
      [1000, 5000, "1-5s"],
      [5000, 10000, "5-10s"],
      [10000, 30000, "10-30s"],
      [30000, Infinity, ">30s"]
    );
  }

  const buckets: DurationBucket[] = bucketRanges.map(([, , range]) => ({
    range,
    count: 0
  }));

  for (const duration of durations) {
    for (let i = 0; i < bucketRanges.length; i++) {
      const [min, max] = bucketRanges[i];
      if (duration >= min && duration < max) {
        buckets[i].count++;
        break;
      }
    }
  }

  return buckets;
}

/**
 * Calculate summary statistics from traces
 */
export function calculateStats(traces: Trace[] | undefined): TraceStats {
  if (!traces || traces.length === 0) {
    return {
      agentRuns: 0,
      workflowRuns: 0,
      avgDuration: "0ms",
      totalTokens: 0
    };
  }

  const agentRuns = traces.filter((t) => t.spanName === "agent.run_agent").length;
  const workflowRuns = traces.filter((t) => t.spanName === "workflow.run_workflow").length;
  const durations = traces.map((t) => getDurationMs(t));
  const avgDuration = durations.reduce((a, b) => a + b, 0) / durations.length;
  const totalTokens = traces.reduce((sum, t) => sum + (getTokensTotal(t) ?? 0), 0);

  return {
    agentRuns,
    workflowRuns,
    avgDuration: formatDuration(avgDuration),
    totalTokens
  };
}

import type { TimelineSpan } from "@/services/api/traces";

export interface TraceSpanMetrics {
  /** spanId → self time (ms): the span's own duration minus time in children. */
  selfTimes: Map<string, number>;
  /** spanIds on the critical (longest-pole) path from the root. */
  criticalPath: Set<string>;
}

const spanEnd = (s: TimelineSpan) => s.offsetMs + s.durationMs;

/**
 * Self time ≈ duration − Σ(direct children durations), clamped at 0. This is
 * the standard trace approximation (children are assumed to run within their
 * parent), and is what the waterfall renders as the lighter "in-children" cap.
 */
function computeSelfTimes(spans: TimelineSpan[]): Map<string, number> {
  const byId = new Set(spans.map((s) => s.spanId));
  const childDuration = new Map<string, number>();
  for (const s of spans) {
    if (s.parentSpanId && byId.has(s.parentSpanId)) {
      childDuration.set(s.parentSpanId, (childDuration.get(s.parentSpanId) ?? 0) + s.durationMs);
    }
  }
  const selfTimes = new Map<string, number>();
  for (const s of spans) {
    selfTimes.set(s.spanId, Math.max(0, s.durationMs - (childDuration.get(s.spanId) ?? 0)));
  }
  return selfTimes;
}

/**
 * Critical path = walk from the latest-finishing root, at each step following
 * the child that finishes last. That chain is what determines the trace's wall
 * time; highlighting it points at the real bottleneck.
 */
function computeCriticalPath(spans: TimelineSpan[]): Set<string> {
  const path = new Set<string>();
  if (spans.length === 0) return path;

  const byId = new Set(spans.map((s) => s.spanId));
  const roots = spans.filter((s) => !s.parentSpanId || !byId.has(s.parentSpanId));
  const seed = roots.length > 0 ? roots : spans;

  let node: TimelineSpan | undefined = seed.reduce((a, b) => (spanEnd(b) > spanEnd(a) ? b : a));
  while (node) {
    path.add(node.spanId);
    const children = spans.filter((s) => s.parentSpanId === node?.spanId);
    if (children.length === 0) break;
    node = children.reduce((a, b) => (spanEnd(b) > spanEnd(a) ? b : a));
  }
  return path;
}

export function computeTraceSpanMetrics(spans: TimelineSpan[]): TraceSpanMetrics {
  return {
    selfTimes: computeSelfTimes(spans),
    criticalPath: computeCriticalPath(spans)
  };
}

/**
 * Share of wall time spent doing work *on* the critical path (0–100). Used by
 * the "Self / crit" summary tile.
 */
export function criticalSelfPercent(
  metrics: TraceSpanMetrics,
  totalDurationMs: number
): number | undefined {
  if (!totalDurationMs) return undefined;
  let critical = 0;
  for (const spanId of metrics.criticalPath) {
    critical += metrics.selfTimes.get(spanId) ?? 0;
  }
  return Math.round((critical / totalDurationMs) * 100);
}

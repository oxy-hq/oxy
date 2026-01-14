import {
  TracesService,
  TraceDetailSpan,
  TimelineSpan,
  SpanEvent,
  tuplesToRecord,
} from "@/services/api/traces";
import { useQuery } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export interface ProcessedTrace {
  traceId: string;
  spans: TimelineSpan[];
  totalDurationMs: number;
  startTime: string;
  endTime: string;
}

// Convert raw API spans to timeline-ready spans
function processTraceSpans(rawSpans: TraceDetailSpan[]): ProcessedTrace | null {
  if (!rawSpans || rawSpans.length === 0) return null;

  const traceId = rawSpans[0].traceId;

  // Sort by timestamp
  const sortedSpans = [...rawSpans].sort(
    (a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime(),
  );

  const startTime = sortedSpans[0].timestamp;
  const startMs = new Date(startTime).getTime();

  // Build parent-child map
  const childrenMap = new Map<string, string[]>();
  sortedSpans.forEach((span) => {
    if (span.parentSpanId) {
      const siblings = childrenMap.get(span.parentSpanId) || [];
      siblings.push(span.spanId);
      childrenMap.set(span.parentSpanId, siblings);
    }
  });

  // Calculate depths
  const depthMap = new Map<string, number>();
  const calculateDepth = (spanId: string, parentSpanId: string): number => {
    if (!parentSpanId) return 0;
    if (depthMap.has(spanId)) return depthMap.get(spanId)!;

    const parentDepth = depthMap.get(parentSpanId) ?? 0;
    const depth = parentDepth + 1;
    depthMap.set(spanId, depth);
    return depth;
  };

  // First pass: set root depths
  sortedSpans.forEach((span) => {
    if (!span.parentSpanId) {
      depthMap.set(span.spanId, 0);
    }
  });

  // Second pass: calculate child depths
  sortedSpans.forEach((span) => {
    if (span.parentSpanId) {
      calculateDepth(span.spanId, span.parentSpanId);
    }
  });

  // Process spans
  const timelineSpans: TimelineSpan[] = sortedSpans.map((span) => {
    const spanStartMs = new Date(span.timestamp).getTime();
    const durationMs = span.duration / 1_000_000; // nanoseconds to milliseconds
    const offsetMs = spanStartMs - startMs;

    // Process events - convert tuple arrays to records
    // Use span timestamp as event timestamp since Events.Timestamp is complex to handle
    const events: SpanEvent[] = span.eventsName.map((name, i) => ({
      timestamp: span.timestamp,
      name: name || "",
      attributes: tuplesToRecord(span.eventsAttributes[i] || []),
    }));

    return {
      spanId: span.spanId,
      parentSpanId: span.parentSpanId,
      spanName: span.spanName,
      timestamp: span.timestamp,
      durationMs,
      offsetMs,
      depth: depthMap.get(span.spanId) ?? 0,
      statusCode: span.statusCode,
      spanKind: span.spanKind,
      attributes: tuplesToRecord(span.spanAttributes),
      events,
      children: childrenMap.get(span.spanId) || [],
    };
  });

  // Calculate total duration (find the span that ends latest)
  let maxEndMs = 0;
  timelineSpans.forEach((span) => {
    const endMs = span.offsetMs + span.durationMs;
    if (endMs > maxEndMs) maxEndMs = endMs;
  });

  const endTime = sortedSpans[sortedSpans.length - 1].timestamp;

  return {
    traceId,
    spans: timelineSpans,
    totalDurationMs: maxEndMs,
    startTime,
    endTime,
  };
}

const useTraceDetail = (traceId: string, enabled = true) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery<ProcessedTrace | null, Error>({
    queryKey: queryKeys.trace.item(projectId, traceId),
    queryFn: async () => {
      const rawSpans = await TracesService.getTraceDetail(projectId, traceId);
      return processTraceSpans(rawSpans);
    },
    enabled: enabled && !!traceId,
  });
};

export default useTraceDetail;

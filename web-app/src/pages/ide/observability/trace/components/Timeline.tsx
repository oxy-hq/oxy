import { TimelineSpanRow } from "./TimelineSpanRow";
import type { TimelineSpan } from "@/services/api/traces";
import { formatDuration } from "../../utils/index";

interface TimelineProps {
  spans: TimelineSpan[];
  totalDuration: number;
  selectedSpanId?: string;
  onSelectSpan: (span: TimelineSpan) => void;
}

export function Timeline({
  spans,
  totalDuration,
  selectedSpanId,
  onSelectSpan,
}: TimelineProps) {
  // Filter out tool.execute spans that have no events
  const filteredSpans = spans.filter(
    (s) => !(s.spanName === "tool.execute" && s.events.length === 0),
  );
  const rootSpans = filteredSpans.filter((s) => !s.parentSpanId);

  return (
    <div className="min-w-fit">
      {/* Timeline header - sticky */}
      <div className="sticky top-0 bg-background/95 backdrop-blur-sm z-10 border-b">
        <div className="flex items-center py-2 px-4 text-xs text-muted-foreground font-medium">
          <div className="w-6" /> {/* Expand button space */}
          <div className="w-52 flex-shrink-0 pl-2">Span</div>
          <div className="flex-1 flex justify-between px-3 min-w-[200px]">
            <span>0ms</span>
            <span>{formatDuration(totalDuration / 2)}</span>
            <span>{formatDuration(totalDuration)}</span>
          </div>
          <div className="w-20 text-right pr-2">Duration</div>
        </div>
      </div>

      {/* Spans */}
      <div className="py-1">
        {rootSpans.map((span, index) => (
          <TimelineSpanRow
            key={span.spanId}
            span={span}
            spans={filteredSpans}
            totalDuration={totalDuration}
            selectedSpanId={selectedSpanId}
            onSelectSpan={onSelectSpan}
            ancestorHasMoreSiblings={[]}
            isLastChild={index === rootSpans.length - 1}
          />
        ))}
      </div>
    </div>
  );
}

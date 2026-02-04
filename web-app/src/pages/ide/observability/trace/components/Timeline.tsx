import type { TimelineSpan } from "@/services/api/traces";
import { formatDuration } from "../../utils/index";
import { TimelineSpanRow } from "./TimelineSpanRow";

interface TimelineProps {
  spans: TimelineSpan[];
  totalDuration: number;
  selectedSpanId?: string;
  onSelectSpan: (span: TimelineSpan) => void;
}

export function Timeline({ spans, totalDuration, selectedSpanId, onSelectSpan }: TimelineProps) {
  // Filter out tool.execute spans that have no events
  const filteredSpans = spans.filter(
    (s) => !(s.spanName === "tool.execute" && s.events.length === 0)
  );
  const rootSpans = filteredSpans.filter((s) => !s.parentSpanId);

  return (
    <div className='min-w-fit'>
      {/* Timeline header - sticky */}
      <div className='sticky top-0 z-10 border-b bg-background/95 backdrop-blur-sm'>
        <div className='flex items-center px-4 py-2 font-medium text-muted-foreground text-xs'>
          <div className='w-6' /> {/* Expand button space */}
          <div className='w-52 flex-shrink-0 pl-2'>Span</div>
          <div className='flex min-w-[200px] flex-1 justify-between px-3'>
            <span>0ms</span>
            <span>{formatDuration(totalDuration / 2)}</span>
            <span>{formatDuration(totalDuration)}</span>
          </div>
          <div className='w-20 pr-2 text-right'>Duration</div>
        </div>
      </div>

      {/* Spans */}
      <div className='py-1'>
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

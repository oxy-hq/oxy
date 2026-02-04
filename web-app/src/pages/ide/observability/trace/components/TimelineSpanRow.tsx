import { AlertCircle, ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import type { TimelineSpan } from "@/services/api/traces";
import { formatDuration, formatSpanLabel, SpanIcon } from "../../utils/index";
import { getTimelineSpanColor } from "./utils";

interface TimelineSpanRowProps {
  span: TimelineSpan;
  spans: TimelineSpan[];
  totalDuration: number;
  selectedSpanId?: string;
  onSelectSpan: (span: TimelineSpan) => void;
  /** Array of booleans indicating if ancestor at each depth has more siblings after this branch */
  ancestorHasMoreSiblings?: boolean[];
  /** Whether this span is the last child among its siblings */
  isLastChild?: boolean;
}

export function TimelineSpanRow({
  span,
  spans,
  totalDuration,
  selectedSpanId,
  onSelectSpan,
  ancestorHasMoreSiblings = [],
  isLastChild = true
}: TimelineSpanRowProps) {
  const [expanded, setExpanded] = useState(span.depth < 3);
  const children = spans.filter((s) => s.parentSpanId === span.spanId);
  const hasChildren = children.length > 0;
  const isSelected = selectedSpanId === span.spanId;

  const offset = (span.offsetMs / totalDuration) * 100;
  const width = (span.durationMs / totalDuration) * 100;
  const spanColor = getTimelineSpanColor(span.spanName, span.statusCode);

  // Build tree connector lines for ancestors
  const treeConnectors = ancestorHasMoreSiblings.map((hasMore, index) => (
    <div key={index} className='relative h-full w-5 flex-shrink-0'>
      {hasMore && <div className='absolute top-0 bottom-0 left-2.5 w-px bg-border' />}
    </div>
  ));

  return (
    <>
      <div
        className={`group flex cursor-pointer items-center px-2 py-1.5 transition-colors ${
          isSelected
            ? "border-l-2 border-l-primary bg-primary/15"
            : "border-l-2 border-l-transparent hover:bg-accent/50"
        }`}
        onClick={() => {
          onSelectSpan(span);
        }}
      >
        {/* Tree structure with connectors */}
        <div className='flex h-6 flex-shrink-0 items-center'>
          {/* Ancestor vertical lines */}
          {treeConnectors}

          {/* Current level connector */}
          {span.depth > 0 && (
            <div className='relative h-full w-5 flex-shrink-0'>
              {/* Vertical line from top (connects to parent) */}
              <div
                className={`absolute top-0 left-2.5 w-px bg-border ${
                  isLastChild ? "h-1/2" : "h-full"
                }`}
              />
              {/* Horizontal line to node */}
              <div className='absolute top-1/2 left-2.5 h-px w-2.5 bg-border' />
            </div>
          )}

          {/* Expand/collapse button or leaf indicator */}
          <div className='relative flex h-5 w-5 flex-shrink-0 items-center justify-center'>
            {hasChildren ? (
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  setExpanded(!expanded);
                }}
                className='z-10 flex h-4 w-4 items-center justify-center rounded border border-border bg-background transition-colors hover:bg-muted'
              >
                {expanded ? (
                  <ChevronDown className='h-3 w-3 text-muted-foreground' />
                ) : (
                  <ChevronRight className='h-3 w-3 text-muted-foreground' />
                )}
              </button>
            ) : (
              <div className='h-1.5 w-1.5 rounded-full bg-border' />
            )}
          </div>
        </div>

        {/* Span info */}
        <div className='ml-1 flex w-52 flex-shrink-0 items-center gap-2'>
          <SpanIcon
            spanName={span.spanName}
            className='h-4 w-4 flex-shrink-0 text-muted-foreground'
          />
          <span className='truncate font-medium text-sm' title={span.spanName}>
            {formatSpanLabel(span.spanName)}
          </span>
          {/* Duration badge inline */}
          <span className='flex-shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground'>
            {formatDuration(span.durationMs)}
          </span>
          {span.statusCode === "Error" && (
            <AlertCircle className='h-3.5 w-3.5 flex-shrink-0 text-destructive' />
          )}
        </div>

        {/* Timeline bar */}
        <div className='relative h-5 min-w-[200px] flex-1 overflow-hidden rounded bg-muted/30'>
          {/* Grid lines */}
          <div className='pointer-events-none absolute inset-0 flex'>
            <div className='flex-1 border-border/20 border-r' />
            <div className='flex-1 border-border/20 border-r' />
            <div className='flex-1 border-border/20 border-r' />
            <div className='flex-1' />
          </div>
          {/* Span bar */}
          <div
            className={`absolute top-0.5 bottom-0.5 rounded-sm ${spanColor} transition-all`}
            style={{
              left: `${Math.max(0, offset)}%`,
              width: `${Math.max(width, 0.5)}%`,
              minWidth: "3px"
            }}
            title={`${span.spanName}: ${formatDuration(span.durationMs)}`}
          >
            {width > 12 && (
              <span className='absolute inset-0 flex items-center truncate px-1.5 font-medium text-[10px] text-white'>
                {formatDuration(span.durationMs)}
              </span>
            )}
          </div>
        </div>

        {/* Duration column */}
        <div className='w-20 pr-2 text-right font-medium text-muted-foreground text-xs'>
          {formatDuration(span.durationMs)}
        </div>
      </div>

      {/* Children */}
      {expanded &&
        hasChildren &&
        children.map((child, index) => {
          const isLast = index === children.length - 1;
          // Pass down ancestor info: current span's continuation status
          const newAncestorHasMoreSiblings = [...ancestorHasMoreSiblings, !isLastChild];

          return (
            <TimelineSpanRow
              key={child.spanId}
              span={child}
              spans={spans}
              totalDuration={totalDuration}
              selectedSpanId={selectedSpanId}
              onSelectSpan={onSelectSpan}
              ancestorHasMoreSiblings={newAncestorHasMoreSiblings}
              isLastChild={isLast}
            />
          );
        })}
    </>
  );
}

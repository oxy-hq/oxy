import { useState } from "react";
import { ChevronDown, ChevronRight, AlertCircle } from "lucide-react";
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
  isLastChild = true,
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
    <div key={index} className="w-5 h-full flex-shrink-0 relative">
      {hasMore && (
        <div className="absolute left-2.5 top-0 bottom-0 w-px bg-border" />
      )}
    </div>
  ));

  return (
    <>
      <div
        className={`group flex items-center py-1.5 px-2 cursor-pointer transition-colors ${
          isSelected
            ? "bg-primary/15 border-l-2 border-l-primary"
            : "border-l-2 border-l-transparent hover:bg-accent/50"
        }`}
        onClick={() => {
          onSelectSpan(span);
        }}
      >
        {/* Tree structure with connectors */}
        <div className="flex items-center h-6 flex-shrink-0">
          {/* Ancestor vertical lines */}
          {treeConnectors}

          {/* Current level connector */}
          {span.depth > 0 && (
            <div className="w-5 h-full flex-shrink-0 relative">
              {/* Vertical line from top (connects to parent) */}
              <div
                className={`absolute left-2.5 top-0 w-px bg-border ${
                  isLastChild ? "h-1/2" : "h-full"
                }`}
              />
              {/* Horizontal line to node */}
              <div className="absolute left-2.5 top-1/2 w-2.5 h-px bg-border" />
            </div>
          )}

          {/* Expand/collapse button or leaf indicator */}
          <div className="w-5 h-5 flex-shrink-0 flex items-center justify-center relative">
            {hasChildren ? (
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  setExpanded(!expanded);
                }}
                className="w-4 h-4 flex items-center justify-center rounded border border-border bg-background hover:bg-muted transition-colors z-10"
              >
                {expanded ? (
                  <ChevronDown className="h-3 w-3 text-muted-foreground" />
                ) : (
                  <ChevronRight className="h-3 w-3 text-muted-foreground" />
                )}
              </button>
            ) : (
              <div className="w-1.5 h-1.5 rounded-full bg-border" />
            )}
          </div>
        </div>

        {/* Span info */}
        <div className="flex items-center gap-2 w-52 flex-shrink-0 ml-1">
          <SpanIcon
            spanName={span.spanName}
            className="h-4 w-4 flex-shrink-0 text-muted-foreground"
          />
          <span className="text-sm font-medium truncate" title={span.spanName}>
            {formatSpanLabel(span.spanName)}
          </span>
          {/* Duration badge inline */}
          <span className="text-[10px] text-muted-foreground bg-muted px-1.5 py-0.5 rounded flex-shrink-0">
            {formatDuration(span.durationMs)}
          </span>
          {span.statusCode === "Error" && (
            <AlertCircle className="h-3.5 w-3.5 text-destructive flex-shrink-0" />
          )}
        </div>

        {/* Timeline bar */}
        <div className="flex-1 relative h-5 bg-muted/30 rounded overflow-hidden min-w-[200px]">
          {/* Grid lines */}
          <div className="absolute inset-0 flex pointer-events-none">
            <div className="flex-1 border-r border-border/20" />
            <div className="flex-1 border-r border-border/20" />
            <div className="flex-1 border-r border-border/20" />
            <div className="flex-1" />
          </div>
          {/* Span bar */}
          <div
            className={`absolute top-0.5 bottom-0.5 rounded-sm ${spanColor} transition-all`}
            style={{
              left: `${Math.max(0, offset)}%`,
              width: `${Math.max(width, 0.5)}%`,
              minWidth: "3px",
            }}
            title={`${span.spanName}: ${formatDuration(span.durationMs)}`}
          >
            {width > 12 && (
              <span className="absolute inset-0 flex items-center px-1.5 text-[10px] font-medium text-white truncate">
                {formatDuration(span.durationMs)}
              </span>
            )}
          </div>
        </div>

        {/* Duration column */}
        <div className="w-20 text-right text-xs text-muted-foreground font-medium pr-2">
          {formatDuration(span.durationMs)}
        </div>
      </div>

      {/* Children */}
      {expanded && hasChildren && (
        <>
          {children.map((child, index) => {
            const isLast = index === children.length - 1;
            // Pass down ancestor info: current span's continuation status
            const newAncestorHasMoreSiblings = [
              ...ancestorHasMoreSiblings,
              !isLastChild,
            ];

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
      )}
    </>
  );
}

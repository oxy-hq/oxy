import { useCallback, useState } from "react";
import { useParams } from "react-router-dom";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import useTraceDetail from "@/hooks/api/traces/useTraceDetail";
import type { TimelineSpan } from "@/services/api/traces";
import { ErrorPage } from "../components/ErrorPage";
import { SpanDetailPanel } from "./components/SpanDetailPanel";
import { Timeline } from "./components/Timeline";
import { TraceHeader } from "./components/TraceHeader";

export default function TraceDetailPage() {
  const { traceId } = useParams<{ traceId: string }>();
  const { data: trace, isLoading, error } = useTraceDetail(traceId || "");
  const [selectedSpanId, setSelectedSpanId] = useState<string | null>(null);

  // Only show selected span when explicitly clicked (no auto-select)
  const selectedSpan = trace?.spans.find((s) => s.spanId === selectedSpanId) ?? null;

  const handleSelectSpan = useCallback((span: TimelineSpan) => {
    setSelectedSpanId(span.spanId);
  }, []);

  const handleClosePanel = useCallback(() => {
    setSelectedSpanId(null);
  }, []);

  if (isLoading) {
    return (
      <div className='flex h-full items-center justify-center'>
        <div className='text-muted-foreground'>Loading trace...</div>
      </div>
    );
  }

  if (error || !trace) {
    return (
      <ErrorPage message='Failed to load trace' description={error?.message || "Trace not found"} />
    );
  }

  return (
    <div className='flex h-full flex-col bg-background'>
      <TraceHeader
        traceId={traceId || ""}
        totalDurationMs={trace.totalDurationMs}
        spansCount={trace.spans.length}
        startTime={trace.startTime}
      />

      {/* Main Content with Resizable Panels */}
      <div className='flex-1 overflow-hidden'>
        <ResizablePanelGroup direction='horizontal'>
          {/* Timeline Panel */}
          <ResizablePanel
            defaultSize={selectedSpan ? 50 : 100}
            minSize={30}
            className='flex flex-col'
          >
            <div className='customScrollbar scrollbar-gutter-auto flex-1 overflow-auto'>
              <Timeline
                spans={trace.spans}
                totalDuration={trace.totalDurationMs}
                selectedSpanId={selectedSpan?.spanId}
                onSelectSpan={handleSelectSpan}
              />
            </div>
          </ResizablePanel>

          {/* Detail Panel - Only shown when span is selected */}
          {selectedSpan && (
            <>
              <ResizableHandle withHandle />
              <ResizablePanel defaultSize={50} minSize={25} className='flex flex-col'>
                <SpanDetailPanel
                  key={selectedSpan.spanId}
                  span={selectedSpan}
                  onClose={handleClosePanel}
                />
              </ResizablePanel>
            </>
          )}
        </ResizablePanelGroup>
      </div>
    </div>
  );
}

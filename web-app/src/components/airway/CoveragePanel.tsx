/**
 * Coverage view for a single backfill range (the drill-in from the ranges
 * gantt).
 *
 * Renders the range's per-chunk status grid + a rollup (done/total, loaded
 * envelope, missing count) from `GET /agentic-airway/coverage?range_id=…`, and a
 * one-click "Resume missing" that POSTs `/resume-backfill { range_id }` — the
 * server re-runs exactly the range's not-`done` chunks. Polls while any chunk is
 * still in flight (see `useAirwayCoverage`).
 */

import { CheckCircle2, Loader2, RefreshCw } from "lucide-react";
import type React from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/shadcn/button";
import { useAirwayCoverage, useResumeBackfill } from "@/hooks/api/airway/useAirway";

/** Status → swatch color + short label. Unknown statuses fall back to pending. */
const STATUS_STYLE: Record<string, { dot: string; label: string }> = {
  done: { dot: "bg-green-500", label: "done" },
  running: { dot: "bg-blue-500 animate-pulse", label: "running" },
  pending: { dot: "bg-muted-foreground/40", label: "pending" },
  completed_with_errors: { dot: "bg-amber-500", label: "errors" },
  failed: { dot: "bg-red-500", label: "failed" },
  cancelled: { dot: "bg-red-500", label: "cancelled" },
  timed_out: { dot: "bg-red-500", label: "timed out" }
};

const styleFor = (status: string) => STATUS_STYLE[status] ?? STATUS_STYLE.pending;

/** ISO datetime → `YYYY-MM-DD`. */
const day = (iso: string) => iso.slice(0, 10);

const CoveragePanel: React.FC<{ rangeId: string; pipelineRef: string }> = ({
  rangeId,
  pipelineRef
}) => {
  const { data, isLoading, error } = useAirwayCoverage(rangeId);
  const resumeBackfill = useResumeBackfill();

  const resume = async () => {
    if (!data || data.summary.missing === 0) return;
    try {
      // Re-runs exactly this range's not-`done` chunks; pipeline_ref is only
      // used client-side to invalidate the ranges/runs caches.
      const { chunk_count } = await resumeBackfill.mutateAsync({
        range_id: rangeId,
        pipeline_ref: pipelineRef
      });
      toast.success(
        `Resuming — re-running ${chunk_count} missing chunk${chunk_count === 1 ? "" : "s"}`
      );
    } catch (e) {
      toast.error(e instanceof Error ? `Resume failed: ${e.message}` : "Resume failed");
    }
  };

  if (isLoading) {
    return <div className='p-6 text-muted-foreground text-sm'>Loading coverage…</div>;
  }
  if (error) {
    return <div className='p-6 text-destructive text-sm'>Failed to load coverage.</div>;
  }
  if (!data || data.chunks.length === 0) {
    return <div className='p-6 text-muted-foreground text-sm'>This range has no chunks.</div>;
  }

  const { summary, chunks } = data;
  const hasMissing = summary.missing > 0;

  return (
    <div className='flex flex-col gap-4 p-4'>
      {/* Rollup strip */}
      <div className='flex flex-wrap items-center gap-x-6 gap-y-2 text-sm'>
        <span className='font-medium'>
          {summary.done}/{summary.total} chunks done
        </span>
        {summary.loaded_from && summary.loaded_to && (
          <span className='text-muted-foreground'>
            loaded {day(summary.loaded_from)} → {day(summary.loaded_to)}
          </span>
        )}
        {hasMissing ? (
          <span className='text-amber-600'>{summary.missing} period(s) missing</span>
        ) : (
          <span className='flex items-center gap-1 text-green-600'>
            <CheckCircle2 className='h-4 w-4' /> fully covered
          </span>
        )}
        <div className='ml-auto'>
          <Button
            size='sm'
            variant='outline'
            onClick={resume}
            disabled={!hasMissing || resumeBackfill.isPending}
            aria-label='Resume missing chunks'
          >
            {resumeBackfill.isPending ? (
              <Loader2 className='h-4 w-4 animate-spin' />
            ) : (
              <RefreshCw className='h-4 w-4' />
            )}
            Resume missing
          </Button>
        </div>
      </div>

      {/* Chunk grid — one swatch per chunk, hover for detail. */}
      <div className='flex flex-wrap gap-1'>
        {chunks.map((c) => {
          const s = styleFor(c.status);
          const detail =
            `${day(c.period_start)} → ${day(c.period_end)} · ${s.label}` +
            (c.row_count != null ? ` · ${c.row_count.toLocaleString()} rows` : "") +
            (c.attempts ? ` · ${c.attempts} attempt(s)` : "") +
            (c.error ? ` · ${c.error}` : "");
          return (
            <div
              key={`${c.period_start}-${c.period_end}`}
              title={detail}
              className='flex h-6 w-6 items-center justify-center rounded-sm border border-border'
            >
              <span className={`h-3 w-3 rounded-full ${s.dot}`} />
            </div>
          );
        })}
      </div>

      {/* Legend */}
      <div className='flex flex-wrap gap-x-4 gap-y-1 text-muted-foreground text-xs'>
        {["done", "running", "pending", "completed_with_errors", "failed"].map((k) => (
          <span key={k} className='flex items-center gap-1'>
            <span className={`h-2.5 w-2.5 rounded-full ${styleFor(k).dot}`} /> {styleFor(k).label}
          </span>
        ))}
      </div>

      {/* Missing-period detail — the "what's missing" answer. */}
      {hasMissing && (
        <div className='flex flex-col gap-1 text-xs'>
          <span className='text-muted-foreground'>Not done:</span>
          {chunks
            .filter((c) => c.status !== "done")
            .map((c) => (
              <div key={`missing-${c.period_start}`} className='flex items-center gap-2'>
                <span className={`h-2 w-2 shrink-0 rounded-full ${styleFor(c.status).dot}`} />
                <span className='font-mono'>
                  {day(c.period_start)} → {day(c.period_end)}
                </span>
                <span className='text-muted-foreground'>[{c.status}]</span>
                {c.error && <span className='truncate text-destructive'>{c.error}</span>}
              </div>
            ))}
        </div>
      )}
    </div>
  );
};

export default CoveragePanel;

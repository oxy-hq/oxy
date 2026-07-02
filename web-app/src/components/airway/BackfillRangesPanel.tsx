/**
 * Backfill ranges for a pipeline — a gantt of user-initiated backfill windows
 * (from `GET /agentic-airway/backfill-ranges`), newest first, each a bar on a
 * shared time axis colored by its rollup status. Selecting a range drills into
 * its per-chunk coverage (`CoveragePanel`).
 *
 * Replaces the old single flat coverage grid: ranges are kept separate (no
 * merge), so overlapping backfills are distinct, inspectable rows.
 */

import { CheckCircle2, Loader2 } from "lucide-react";
import type React from "react";
import { useEffect, useState } from "react";

import CoveragePanel from "@/components/airway/CoveragePanel";
import { useBackfillRanges } from "@/hooks/api/airway/useAirway";
import { cn } from "@/libs/shadcn/utils";

/** Rollup status → bar color. Unknown falls back to muted. */
const STATUS_BAR: Record<string, string> = {
  running: "bg-blue-500 animate-pulse",
  done: "bg-green-500",
  degraded: "bg-amber-500",
  failed: "bg-red-500",
  cancelled: "bg-muted-foreground/40"
};
const barClass = (status: string) => STATUS_BAR[status] ?? "bg-muted-foreground/40";

/** ISO datetime → `YYYY-MM-DD`. */
const day = (iso: string) => iso.slice(0, 10);
const ms = (iso: string) => Date.parse(iso);

const BackfillRangesPanel: React.FC<{ pipelineRef: string }> = ({ pipelineRef }) => {
  const { data: ranges, isLoading, error } = useBackfillRanges(pipelineRef);
  const [selected, setSelected] = useState<string | undefined>(undefined);

  // Default the drill-in to the newest range once loaded, and re-anchor if the
  // selection falls out of the list (e.g. after a refetch).
  useEffect(() => {
    if (!ranges || ranges.length === 0) return;
    if (!selected || !ranges.some((r) => r.id === selected)) {
      setSelected(ranges[0].id);
    }
  }, [ranges, selected]);

  if (isLoading) {
    return <div className='p-6 text-muted-foreground text-sm'>Loading backfill ranges…</div>;
  }
  if (error) {
    return <div className='p-6 text-destructive text-sm'>Failed to load backfill ranges.</div>;
  }
  if (!ranges || ranges.length === 0) {
    return (
      <div className='p-6 text-muted-foreground text-sm'>
        No chunked backfill has run for this pipeline yet. Start one from{" "}
        <span className='font-medium text-foreground'>Backfill → Chunked (resumable)</span>.
      </div>
    );
  }

  // Shared time axis across every range's window.
  const t0 = Math.min(...ranges.map((r) => ms(r.requested_from)));
  const t1 = Math.max(...ranges.map((r) => ms(r.requested_to)));
  const span = Math.max(1, t1 - t0);
  const pct = (t: number) => ((t - t0) / span) * 100;

  return (
    <div className='flex flex-col gap-4 p-4'>
      <div className='text-muted-foreground text-xs'>
        {ranges.length} backfill range{ranges.length === 1 ? "" : "s"} · select one to see its
        chunks
      </div>

      {/* Gantt: one row per range, bar positioned on the shared axis. */}
      <div className='flex flex-col gap-1'>
        {ranges.map((r) => {
          const left = pct(ms(r.requested_from));
          const width = Math.max(1.5, pct(ms(r.requested_to)) - left);
          const isSel = r.id === selected;
          return (
            <button
              key={r.id}
              type='button'
              onClick={() => setSelected(r.id)}
              aria-pressed={isSel}
              className={cn(
                "flex items-center gap-3 rounded-md px-2 py-1 text-left transition-colors",
                isSel ? "bg-muted" : "hover:bg-muted/50"
              )}
            >
              <span className='w-44 shrink-0 font-mono text-xs'>
                {day(r.requested_from)} → {day(r.requested_to)}
              </span>
              <span className='relative h-4 flex-1 rounded-sm bg-muted/40'>
                <span
                  className={cn("absolute top-0 h-4 rounded-sm", barClass(r.status))}
                  style={{ left: `${left.toFixed(2)}%`, width: `${width.toFixed(2)}%` }}
                  title={`${r.status} · ${r.done}/${r.total} chunks · ${r.granularity}`}
                />
              </span>
              <span className='flex w-24 shrink-0 items-center justify-end gap-1 text-xs'>
                {r.status === "done" ? (
                  <CheckCircle2 className='h-3.5 w-3.5 text-green-600' />
                ) : r.status === "running" ? (
                  <Loader2 className='h-3.5 w-3.5 animate-spin text-blue-500' />
                ) : null}
                <span className='text-muted-foreground'>
                  {r.done}/{r.total}
                </span>
              </span>
            </button>
          );
        })}
      </div>

      {/* Drill-in: the selected range's per-chunk coverage. */}
      {selected && (
        <div className='rounded-md border border-border'>
          <CoveragePanel rangeId={selected} pipelineRef={pipelineRef} />
        </div>
      )}
    </div>
  );
};

export default BackfillRangesPanel;

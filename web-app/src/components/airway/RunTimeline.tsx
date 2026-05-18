/**
 * Run timeline — a per-resource Gantt of a single run (Dagster/Prefect
 * run-view shape). Presentation only, from `AirwayRunView`: each
 * resource is a track with extract → normalize → load segments
 * positioned on a shared wall-clock axis (timestamps come from the
 * event `ts` the worker stamps at emit; the reducer captures them).
 */

import type React from "react";
import { cn } from "@/libs/shadcn/utils";
import type { AirwayRunView, ResourceRow } from "@/utils/airwayReducer";

const ms = (iso?: string): number | undefined => {
  if (!iso) return undefined;
  const t = Date.parse(iso);
  return Number.isNaN(t) ? undefined : t;
};

const leafLabel = (table: string) => {
  const i = table.lastIndexOf("__");
  return i >= 0 ? table.slice(i + 2) : table;
};

const fmtDur = (msTotal: number) => {
  if (msTotal < 1000) return `${Math.round(msTotal)}ms`;
  const s = msTotal / 1000;
  if (s < 60) return `${s.toFixed(1)}s`;
  const m = Math.floor(s / 60);
  return `${m}m ${Math.round(s % 60)}s`;
};

type Seg = { label: string; cls: string; start?: number; end?: number };

const segsOf = (r: ResourceRow): Seg[] => [
  {
    label: "extract",
    cls: "bg-primary/40",
    start: ms(r.extractStartedAt),
    end: ms(r.extractEndedAt)
  },
  {
    label: "normalize",
    cls: "bg-primary/70",
    start: ms(r.normalizeStartedAt),
    end: ms(r.normalizeEndedAt)
  },
  { label: "load", cls: "bg-primary", start: ms(r.loadStartedAt), end: ms(r.loadEndedAt) }
];

export const RunTimeline: React.FC<{ view: AirwayRunView }> = ({ view }) => {
  const rows = view.resources;

  // Axis bounds: prefer the run span; fall back to the min/max of any
  // captured segment so an in-flight run still renders.
  const stamps: number[] = [];
  for (const r of rows) {
    for (const s of segsOf(r)) {
      if (s.start != null) stamps.push(s.start);
      if (s.end != null) stamps.push(s.end);
    }
  }
  const t0 = ms(view.startedAt) ?? (stamps.length ? Math.min(...stamps) : undefined);
  const t1 = ms(view.endedAt) ?? (stamps.length ? Math.max(...stamps) : undefined);

  if (rows.length === 0 || t0 == null || t1 == null || t1 <= t0) {
    return (
      <div className='px-4 py-10 text-center text-muted-foreground text-sm'>
        Timeline appears once the run emits timed extract/normalize/load events.
      </div>
    );
  }

  const span = t1 - t0;
  const pct = (t: number) => `${(((t - t0) / span) * 100).toFixed(2)}%`;

  return (
    <div className='space-y-1 p-4'>
      <div className='mb-2 text-muted-foreground text-xs'>
        Total {fmtDur(view.durationMs ?? span)} ·{" "}
        {view.status === "running" ? "in progress" : view.status}
      </div>
      {rows.map((r) => {
        const isChild = !!r.parent;
        return (
          <div key={`${r.parent ?? ""}:${r.table}`} className='flex items-center gap-2'>
            <div
              className={cn(
                "w-48 shrink-0 truncate font-mono text-xs",
                isChild ? "pl-4 text-muted-foreground" : "font-medium"
              )}
              title={r.table}
            >
              {isChild ? `└ ${leafLabel(r.table)}` : r.table}
            </div>
            <div className='relative h-5 flex-1 rounded-sm bg-muted/40'>
              {segsOf(r).map((s) => {
                if (s.start == null) return null;
                // In-flight segment (started, no end) runs to the axis edge.
                const end = s.end ?? t1;
                const left = pct(Math.max(s.start, t0));
                const width = `${Math.max(0.5, ((end - Math.max(s.start, t0)) / span) * 100).toFixed(2)}%`;
                return (
                  <div
                    key={s.label}
                    className={cn(
                      "absolute top-0 h-5 rounded-sm",
                      s.cls,
                      s.end == null && "animate-pulse"
                    )}
                    style={{ left, width }}
                    title={`${s.label}: ${fmtDur(end - s.start)}${s.end == null ? " (running)" : ""}`}
                  />
                );
              })}
            </div>
          </div>
        );
      })}
      <div className='mt-2 flex gap-4 text-[10px] text-muted-foreground'>
        <span className='flex items-center gap-1'>
          <span className='inline-block h-2 w-3 rounded-sm bg-primary/40' /> extract
        </span>
        <span className='flex items-center gap-1'>
          <span className='inline-block h-2 w-3 rounded-sm bg-primary/70' /> normalize
        </span>
        <span className='flex items-center gap-1'>
          <span className='inline-block h-2 w-3 rounded-sm bg-primary' /> load
        </span>
      </div>
    </div>
  );
};

export default RunTimeline;

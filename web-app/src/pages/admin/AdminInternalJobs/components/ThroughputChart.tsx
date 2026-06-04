import { useEffect, useMemo, useRef, useState } from "react";
import { cn } from "@/libs/utils/cn";
import { type HistorySample, throughputSeries } from "../useInternalJobsHistory";

/**
 * Multi-series area chart of completed-vs-failed deltas between
 * consecutive 5s snapshots. Pure SVG; no chart library. Shows the
 * pulse of the worker fleet at a glance: a tall green band means
 * tasks are draining, a tall amber band means the failure rate is
 * spiking.
 *
 * Cursor hover surfaces the per-tick value via a vertical guide +
 * tooltip. Width is responsive (100%); height is fixed for predictable
 * layout. The aspect ratio is intentionally short so the chart frames
 * the page without dominating it.
 */
const HEIGHT = 160;
const PAD_T = 12;
const PAD_B = 18;
const PAD_X = 8;

export const ThroughputChart = ({
  history,
  className
}: {
  history: HistorySample[];
  className?: string;
}) => {
  const data = useMemo(() => throughputSeries(history), [history]);
  const [hoverIdx, setHoverIdx] = useState<number | null>(null);
  // Use a ResizeObserver on a stable container ref so the chart reflows
  // on window-resize, not just on the 5s data tick. The previous
  // approach (inline ref callback + setWidth) detached / reattached on
  // every render and never fired between polls.
  //
  // The "skip if unchanged" comparison reads from a ref instead of the
  // `width` state so the effect has no dependencies — preventing the
  // linter from adding `width` to the deps (which would recreate the
  // observer every time it fires) and keeping the observer alive for
  // the lifetime of the component.
  const containerRef = useRef<HTMLDivElement | null>(null);
  const widthRef = useRef(800);
  const [width, setWidth] = useState(800);
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const initial = el.clientWidth;
    if (initial && initial !== widthRef.current) {
      widthRef.current = initial;
      setWidth(initial);
    }
    const observer = new ResizeObserver((entries) => {
      const next = Math.round(entries[0]?.contentRect.width ?? 0);
      if (next && next !== widthRef.current) {
        widthRef.current = next;
        setWidth(next);
      }
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const maxY = Math.max(1, ...data.map((d) => d.completed + d.failed));
  const innerW = Math.max(1, width - PAD_X * 2);
  const innerH = HEIGHT - PAD_T - PAD_B;

  const xFor = (i: number) =>
    PAD_X + (data.length <= 1 ? innerW / 2 : (i / (data.length - 1)) * innerW);
  const yFor = (v: number) => PAD_T + innerH - (v / maxY) * innerH;

  const completedPath = pathFor(
    data.map((d) => d.completed),
    xFor,
    yFor,
    true
  );
  const failedPath = pathFor(
    data.map((d) => d.failed),
    xFor,
    yFor,
    true
  );

  const completedTotal = data.reduce((s, d) => s + d.completed, 0);
  const failedTotal = data.reduce((s, d) => s + d.failed, 0);

  const hovered = hoverIdx !== null && data[hoverIdx] ? data[hoverIdx] : null;

  return (
    <div className={cn("flex flex-col gap-3", className)}>
      <div className='flex items-baseline justify-between'>
        <div>
          <h3 className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
            Throughput (last 5 min)
          </h3>
          <p className='mt-0.5 text-muted-foreground text-xs'>
            Δ completed (emerald) vs Δ failed + dead (amber) between 5s polls.
          </p>
        </div>
        <div className='flex items-center gap-4 font-medium text-[11px] tabular-nums'>
          <span className='inline-flex items-center gap-1.5 text-emerald-700 dark:text-emerald-400'>
            <span className='size-2 rounded-sm bg-emerald-500' aria-hidden />+{completedTotal} done
          </span>
          <span className='inline-flex items-center gap-1.5 text-amber-700 dark:text-amber-400'>
            <span className='size-2 rounded-sm bg-amber-500' aria-hidden />+{failedTotal} failed
          </span>
        </div>
      </div>

      <div
        ref={containerRef}
        className='relative w-full overflow-hidden rounded-lg border border-border/60 bg-card'
      >
        {data.length < 2 ? (
          <div className='flex h-[160px] items-center justify-center text-muted-foreground text-xs'>
            Sampling… give it a few seconds for the chart to populate.
          </div>
        ) : (
          <svg
            viewBox={`0 0 ${width} ${HEIGHT}`}
            width='100%'
            height={HEIGHT}
            onMouseLeave={() => setHoverIdx(null)}
            onMouseMove={(e) => {
              const rect = e.currentTarget.getBoundingClientRect();
              const x = ((e.clientX - rect.left) / rect.width) * width;
              const idx = Math.round(((x - PAD_X) / innerW) * (data.length - 1));
              setHoverIdx(Math.max(0, Math.min(data.length - 1, idx)));
            }}
          >
            {/* Subtle gridlines (3 horizontal). */}
            {[0.25, 0.5, 0.75].map((p) => (
              <line
                key={p}
                x1={PAD_X}
                x2={width - PAD_X}
                y1={PAD_T + innerH * p}
                y2={PAD_T + innerH * p}
                stroke='currentColor'
                className='text-border'
                strokeDasharray='2 4'
              />
            ))}
            {/* Failed under completed so the larger area sits behind. */}
            <path d={failedPath} className='fill-amber-500/15 stroke-amber-500' strokeWidth={1.5} />
            <path
              d={completedPath}
              className='fill-emerald-500/15 stroke-emerald-500'
              strokeWidth={1.5}
            />
            {/* Hover guide. */}
            {hoverIdx !== null && data[hoverIdx] ? (
              <line
                x1={xFor(hoverIdx)}
                x2={xFor(hoverIdx)}
                y1={PAD_T}
                y2={HEIGHT - PAD_B}
                stroke='currentColor'
                className='text-foreground/40'
                strokeWidth={1}
              />
            ) : null}
            {/* X-axis baseline. */}
            <line
              x1={PAD_X}
              x2={width - PAD_X}
              y1={HEIGHT - PAD_B}
              y2={HEIGHT - PAD_B}
              stroke='currentColor'
              className='text-border'
            />
            {/* X-axis labels — first, middle, last timestamps. */}
            {[0, Math.floor(data.length / 2), data.length - 1].map((i) => (
              <text
                key={i}
                x={xFor(i)}
                y={HEIGHT - 4}
                fontSize={10}
                textAnchor='middle'
                className='fill-muted-foreground tabular-nums'
              >
                {fmtClock(data[i].t)}
              </text>
            ))}
          </svg>
        )}

        {hovered ? (
          <div
            className='pointer-events-none absolute top-2 rounded-md border border-border/60 bg-popover px-2 py-1.5 text-popover-foreground text-xs shadow-md'
            style={{
              left: Math.min(width - 140, Math.max(8, xFor(hoverIdx ?? 0) - 70))
            }}
          >
            <div className='font-mono text-[10px] text-muted-foreground tabular-nums'>
              {fmtClock(hovered.t)}
            </div>
            <div className='mt-0.5 flex items-center gap-2 tabular-nums'>
              <span className='inline-flex items-center gap-1 text-emerald-700 dark:text-emerald-400'>
                <span className='size-1.5 rounded-sm bg-emerald-500' aria-hidden />
                {hovered.completed}
              </span>
              <span className='inline-flex items-center gap-1 text-amber-700 dark:text-amber-400'>
                <span className='size-1.5 rounded-sm bg-amber-500' aria-hidden />
                {hovered.failed}
              </span>
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
};

function pathFor(
  values: number[],
  xFor: (i: number) => number,
  yFor: (v: number) => number,
  closed: boolean
): string {
  if (values.length === 0) return "";
  const line = values
    .map((v, i) => `${i === 0 ? "M" : "L"}${xFor(i).toFixed(1)},${yFor(v).toFixed(1)}`)
    .join(" ");
  if (!closed) return line;
  return `${line} L${xFor(values.length - 1).toFixed(1)},${yFor(0).toFixed(1)} L${xFor(0).toFixed(1)},${yFor(0).toFixed(1)} Z`;
}

function fmtClock(t: number): string {
  const d = new Date(t);
  return `${d.getHours().toString().padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}:${d.getSeconds().toString().padStart(2, "0")}`;
}

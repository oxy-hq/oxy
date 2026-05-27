import type React from "react";
import { formatDurationMs } from "../../../components/utils";

/**
 * Shared time-axis strip used above proportional-bar layouts (workflow
 * Graph, ELT tables). 5 evenly-spaced tick labels in elapsed-time
 * format so the reader can map any bar's position to an absolute
 * duration.
 *
 * `leftSlot` / `rightSlot` widths reserve space matching the columns
 * that sit on either side of the bar track below the axis — passing
 * `w-44` here aligns the first tick with the start of the bar track
 * exactly. Defaults match the workflow-graph card layout.
 */
export const TimeAxis: React.FC<{
  spanMs: number;
  /** Tailwind width class for the left gutter. Defaults to `w-44`. */
  leftSlot?: string;
  /** Tailwind width class for the right gutter. Defaults to `w-16`. */
  rightSlot?: string;
}> = ({ spanMs, leftSlot = "w-44", rightSlot = "w-16" }) => {
  const ticks = [0, 0.25, 0.5, 0.75, 1].map((frac) => ({
    pct: frac * 100,
    label: formatDurationMs(spanMs * frac)
  }));
  return (
    <div className='mb-2 flex items-center gap-2'>
      <span className={`${leftSlot} shrink-0`} />
      <div className='relative h-4 flex-1'>
        {ticks.map((tick) => (
          <span
            key={tick.pct}
            className='absolute top-0 -translate-x-1/2 text-muted-foreground text-xs tabular-nums'
            style={{ left: `${tick.pct}%` }}
          >
            {tick.label}
          </span>
        ))}
      </div>
      <span className={`${rightSlot} shrink-0`} />
    </div>
  );
};

import { ArrowDownRight, ArrowUpRight, Minus } from "lucide-react";
import { cn } from "@/libs/utils/cn";
import type { QueueStatusCounts } from "@/services/api/internalJobs";
import type { HistorySample } from "../useInternalJobsHistory";
import { statusSeries } from "../useInternalJobsHistory";
import { Sparkline } from "./Sparkline";

/**
 * Per-status KPI tile + sparkline + delta arrow. Six tiles laid out
 * across an equal-width grid form the operator's first read of the
 * page: queue depth NOW, where it's been the last few minutes, and
 * which direction it's heading.
 *
 * The delta is computed from the first sample in the buffer to the
 * latest — so as the buffer warms up (right after page load) it
 * naturally shows "since you opened the page". Once the buffer is
 * full (~5 min) it shows the last-5-min trend.
 *
 * The 1h / 24h secondary lines remain because operators look at them
 * during incident response ("what changed overnight?").
 */
export const KpiStrip = ({
  total,
  last_1h,
  last_24h,
  history
}: {
  total: QueueStatusCounts;
  last_1h: QueueStatusCounts;
  last_24h: QueueStatusCounts;
  history: HistorySample[];
}) => (
  <div className='grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-6'>
    {STATUSES.map(({ key, label, tone, sparkClass }) => {
      const series = statusSeries(history, key);
      const value = total[key];
      const delta = series.length >= 2 ? series[series.length - 1] - series[0] : 0;
      return (
        <div
          key={key}
          // Tiles are read-only — no inspector filter is wired yet. Stays
          // a plain div (not a button) so screen readers / keyboard users
          // don't get a focusable control that does nothing on Enter.
          className={cn("flex flex-col gap-2 rounded-lg border border-border/60 bg-card p-3")}
        >
          <div className='flex items-center justify-between'>
            <span className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
              {label}
            </span>
            <DeltaPill delta={delta} />
          </div>
          <div className='flex items-end justify-between gap-2'>
            <span className={cn("font-semibold text-2xl tabular-nums tracking-tight", tone)}>
              {value.toLocaleString()}
            </span>
            <Sparkline data={series} toneClass={sparkClass} />
          </div>
          <div className='grid grid-cols-2 gap-1 text-[10px] text-muted-foreground tabular-nums'>
            <span>1h · {last_1h[key].toLocaleString()}</span>
            <span>24h · {last_24h[key].toLocaleString()}</span>
          </div>
        </div>
      );
    })}
  </div>
);

const DeltaPill = ({ delta }: { delta: number }) => {
  if (delta === 0) {
    return (
      <span className='inline-flex items-center gap-0.5 text-[10px] text-muted-foreground tabular-nums'>
        <Minus className='size-3' />0
      </span>
    );
  }
  const positive = delta > 0;
  const Icon = positive ? ArrowUpRight : ArrowDownRight;
  return (
    <span
      className={cn(
        "inline-flex items-center gap-0.5 font-medium text-[10px] tabular-nums",
        positive ? "text-amber-700 dark:text-amber-400" : "text-emerald-700 dark:text-emerald-400"
      )}
    >
      <Icon className='size-3' />
      {positive ? "+" : ""}
      {delta}
    </span>
  );
};

const STATUSES: Array<{
  key: keyof QueueStatusCounts;
  label: string;
  /** Tailwind text class for the big number. */
  tone: string;
  /** Tailwind text class for the sparkline (via `currentColor`). */
  sparkClass: string;
}> = [
  { key: "queued", label: "Queued", tone: "text-foreground", sparkClass: "text-foreground/70" },
  { key: "claimed", label: "Claimed", tone: "text-primary", sparkClass: "text-primary" },
  {
    key: "completed",
    label: "Completed",
    tone: "text-emerald-700 dark:text-emerald-400",
    sparkClass: "text-emerald-700 dark:text-emerald-400"
  },
  {
    key: "failed",
    label: "Failed",
    tone: "text-amber-700 dark:text-amber-400",
    sparkClass: "text-amber-700 dark:text-amber-400"
  },
  {
    key: "cancelled",
    label: "Cancelled",
    tone: "text-muted-foreground",
    sparkClass: "text-muted-foreground"
  },
  { key: "dead", label: "Dead", tone: "text-destructive", sparkClass: "text-destructive" }
];

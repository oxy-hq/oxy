import type { DayCost } from "@/services/api/adminMetrics";
import { usd } from "./format";

/**
 * Compact daily-cost bar chart, custom SVG (no chart lib) to stay on the
 * cockpit aesthetic. Bars scale to the max day; hover shows the exact
 * date + cost + run count. Empty days render as a faint baseline tick so
 * gaps read as "no spend", not "missing data".
 */
export const CostBars = ({ data }: { data: DayCost[] }) => {
  if (data.length === 0) {
    return (
      <div className='flex h-28 items-center justify-center rounded-md border border-border/60 border-dashed text-muted-foreground text-xs'>
        No LLM activity in this window.
      </div>
    );
  }

  const max = Math.max(...data.map((d) => d.cost_usd), 0.0000001);
  const peak = data.reduce((a, b) => (b.cost_usd > a.cost_usd ? b : a), data[0]);

  return (
    <div className='space-y-1.5'>
      <div className='flex h-28 items-end gap-px'>
        {data.map((d) => {
          const pct = Math.max((d.cost_usd / max) * 100, d.cost_usd > 0 ? 4 : 1.5);
          return (
            <div
              key={d.day}
              className='group relative flex flex-1 items-end'
              style={{ height: "100%" }}
              title={`${d.day} · ${usd(d.cost_usd)} · ${d.run_count} run${d.run_count === 1 ? "" : "s"}`}
            >
              <div
                className={
                  d.cost_usd > 0
                    ? "w-full rounded-t-sm bg-primary/70 transition-colors group-hover:bg-primary"
                    : "w-full rounded-t-sm bg-muted-foreground/20"
                }
                style={{ height: `${pct}%` }}
              />
            </div>
          );
        })}
      </div>
      <div className='flex items-center justify-between font-mono text-[10px] text-muted-foreground tabular-nums'>
        <span>{data[0].day}</span>
        <span className='text-foreground/70'>peak {usd(peak.cost_usd)}</span>
        <span>{data[data.length - 1].day}</span>
      </div>
    </div>
  );
};

import { useState } from "react";
import { Link } from "react-router-dom";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useLlmUsage } from "@/hooks/api/adminMetrics/useLlmUsage";
import { cn } from "@/libs/utils/cn";
import ROUTES from "@/libs/utils/routes";
import type { LlmUsageOverview } from "@/services/api/adminMetrics";
import { CostBars } from "./CostBars";
import { compact, shortModel, usd } from "./format";

const WINDOWS = [7, 30, 90] as const;

/**
 * The LLM cost command center — the headline of the operator dashboard.
 * Total spend + daily trend across every tenant, broken down by model and by
 * top-spending org, with a 7/30/90-day window. Dollar figures are computed
 * server-side from per-model rates over the token usage in `agentic_run_events`.
 */
export const LlmCostSection = () => {
  const [days, setDays] = useState<number>(30);
  const { data, isPending, isError } = useLlmUsage(days);

  return (
    <section aria-label='LLM cost' className='space-y-4'>
      <header className='flex flex-wrap items-center justify-between gap-3'>
        <div className='flex items-baseline gap-3'>
          <h2 className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.18em]'>
            LLM cost &amp; usage
          </h2>
          {data ? (
            <span className='font-mono text-[11px] text-muted-foreground tabular-nums'>
              {data.total.run_count.toLocaleString()} runs · last {days}d
            </span>
          ) : null}
        </div>
        <div className='flex items-center gap-1'>
          {WINDOWS.map((w) => (
            <button
              key={w}
              type='button'
              onClick={() => setDays(w)}
              className={cn(
                "rounded-md px-2 py-1 font-medium text-[11px] uppercase tracking-wide transition-colors",
                days === w
                  ? "bg-foreground text-background"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground"
              )}
            >
              {w}d
            </button>
          ))}
        </div>
      </header>

      {isPending ? (
        <Skeleton className='h-64 w-full' />
      ) : isError || !data ? (
        <div className='rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-destructive text-xs'>
          Failed to load LLM usage.
        </div>
      ) : (
        <Body data={data} days={days} />
      )}
    </section>
  );
};

const Body = ({ data, days }: { data: LlmUsageOverview; days: number }) => {
  const t = data.total;
  const unpriced = t.run_count - t.priced_run_count;
  return (
    <div className='grid gap-4 lg:grid-cols-3'>
      {/* Spend + trend — the dominant card */}
      <div className='space-y-4 rounded-lg border border-border/60 bg-card p-4 lg:col-span-2'>
        <div className='flex flex-wrap items-end justify-between gap-4'>
          <div>
            <span className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
              Total spend · {days}d
            </span>
            <div className='font-semibold text-2xl tabular-nums tracking-tight'>
              {usd(t.cost_usd)}
            </div>
          </div>
          <div className='flex gap-5 text-right'>
            <Metric label='Tokens in' value={compact(t.input_tokens)} />
            <Metric label='Tokens out' value={compact(t.output_tokens)} />
            <Metric label='Cache read' value={compact(t.cache_read_tokens)} />
          </div>
        </div>
        <CostBars data={data.by_day} />
        {unpriced > 0 ? (
          <p className='text-[10px] text-muted-foreground'>
            {unpriced.toLocaleString()} run{unpriced === 1 ? "" : "s"} used a model with no known
            price — tokens counted, dollars excluded.
          </p>
        ) : null}
      </div>

      {/* By model */}
      <div className='space-y-3 rounded-lg border border-border/60 bg-card p-4'>
        <span className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
          By model
        </span>
        {data.by_model.length === 0 ? (
          <p className='text-muted-foreground text-xs'>No usage.</p>
        ) : (
          <div className='space-y-2.5'>
            {data.by_model.slice(0, 6).map((m) => {
              const share = t.cost_usd > 0 ? ((m.cost_usd ?? 0) / t.cost_usd) * 100 : 0;
              return (
                <div key={m.model} className='space-y-1'>
                  <div className='flex items-center justify-between gap-2 text-xs'>
                    <span className='truncate font-mono'>{shortModel(m.model)}</span>
                    <span className='shrink-0 tabular-nums'>
                      {m.cost_usd === null ? "—" : usd(m.cost_usd)}
                    </span>
                  </div>
                  <div className='h-1 w-full overflow-hidden rounded-full bg-muted'>
                    <div
                      className='h-full rounded-full bg-primary/70'
                      style={{ width: `${Math.max(share, m.cost_usd ? 3 : 0)}%` }}
                    />
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Top orgs by spend — full width below */}
      <div className='space-y-2 rounded-lg border border-border/60 bg-card p-4 lg:col-span-3'>
        <span className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
          Top accounts by spend
        </span>
        {data.by_org.length === 0 ? (
          <p className='text-muted-foreground text-xs'>
            No tenant-attributed usage in this window.
          </p>
        ) : (
          <div className='divide-y divide-border/50'>
            {data.by_org.map((o, i) => (
              <Link
                key={o.org_id}
                to={ROUTES.ADMIN.ORG_DETAIL(o.org_id)}
                className='flex items-center gap-3 py-1.5 text-xs transition-colors hover:bg-muted/30'
              >
                <span className='w-5 shrink-0 text-right font-mono text-muted-foreground tabular-nums'>
                  {i + 1}
                </span>
                <span className='min-w-0 flex-1 truncate font-medium'>{o.org_name}</span>
                <span className='shrink-0 font-mono text-muted-foreground tabular-nums'>
                  {o.run_count.toLocaleString()} runs
                </span>
                <span className='w-20 shrink-0 text-right font-semibold tabular-nums'>
                  {usd(o.cost_usd)}
                </span>
              </Link>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};

const Metric = ({ label, value }: { label: string; value: string }) => (
  <div>
    <span className='block font-medium text-[10px] text-muted-foreground uppercase tracking-[0.12em]'>
      {label}
    </span>
    <span className='font-mono text-xs tabular-nums'>{value}</span>
  </div>
);

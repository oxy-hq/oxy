import { Activity, Cpu } from "lucide-react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useWorkers } from "@/hooks/api/internalJobs";
import { cn } from "@/libs/utils/cn";
import { relativeTime } from "../../utils";
import { useLive } from "../LiveContext";

/**
 * Worker fleet — one card per `worker_id` seen in the last 24h. Liveness
 * is derived from how recent the last claim was: green dot if < 60s,
 * amber if < 5min, red after. This is the "Busy" tab from Sidekiq's
 * Web UI: at a glance the operator knows which workers are drawing
 * tasks right now and which are stale.
 *
 * Inflight count is rendered as a bar so a "busy worker" reads
 * visually, not just as a number. Layout is a horizontal-scrolling
 * rail when there are many workers — typical fleets have 2-8 so the
 * default grid is fine.
 */
export const WorkerFleetPanel = () => {
  const { paused } = useLive();
  const { data, isLoading, isError } = useWorkers({ paused });
  // Hoisted out of the worker .map so the busiest-worker scan is O(n)
  // instead of O(n²). Trivial at typical fleet sizes (2-8) but cheap.
  const inflightMax = data?.workers.length
    ? Math.max(...data.workers.map((w) => w.inflight_count), 1)
    : 1;

  return (
    <section className='space-y-3'>
      <header className='flex items-baseline justify-between'>
        <h3 className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
          Worker fleet
        </h3>
        {data?.workers ? (
          <span className='font-medium text-[11px] text-muted-foreground tabular-nums'>
            {data.workers.length} {data.workers.length === 1 ? "worker" : "workers"}
          </span>
        ) : null}
      </header>

      {isLoading ? (
        <Skeleton className='h-24 w-full' />
      ) : isError || !data ? (
        <div className='rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-destructive text-xs'>
          Failed to load worker fleet.
        </div>
      ) : !data.supported ? (
        <div className='rounded-lg border border-border/60 border-dashed bg-muted/20 p-6 text-center text-muted-foreground text-xs'>
          Worker fleet info isn't available for this deployment (column not present).
        </div>
      ) : data.workers.length === 0 ? (
        <div className='rounded-lg border border-border/60 border-dashed bg-muted/20 p-6 text-center text-muted-foreground text-xs'>
          No worker activity in the last 24h. Either the queue has been idle, or no `oxy worker`
          process is connected.
        </div>
      ) : (
        <div className='grid grid-cols-1 gap-2 md:grid-cols-2 xl:grid-cols-3'>
          {data.workers.map((w) => {
            const tone = livenessTone(w.last_claim_at);
            const inflightPct = (w.inflight_count / inflightMax) * 100;
            return (
              <div
                key={w.worker_id}
                className='flex flex-col gap-2 rounded-lg border border-border/60 bg-card p-3'
              >
                <div className='flex items-center justify-between gap-2'>
                  <div className='flex min-w-0 items-center gap-2'>
                    <span
                      className={cn("size-2 shrink-0 rounded-full", tone.dotClass)}
                      aria-hidden
                    />
                    <span className='truncate font-mono text-[11px]'>{w.worker_id}</span>
                  </div>
                  <span
                    className={cn(
                      "font-medium text-[10px] uppercase tracking-wide",
                      tone.labelClass
                    )}
                  >
                    {tone.label}
                  </span>
                </div>
                <div className='flex items-center gap-2 text-[11px] text-muted-foreground tabular-nums'>
                  <Activity className='size-3' />
                  Last claim {w.last_claim_at ? relativeTime(w.last_claim_at) : "—"}
                </div>
                <div className='space-y-1'>
                  <div className='flex items-center justify-between text-[10px] text-muted-foreground'>
                    <span className='inline-flex items-center gap-1 uppercase tracking-wide'>
                      <Cpu className='size-3' />
                      Inflight
                    </span>
                    <span className='font-medium text-foreground tabular-nums'>
                      {w.inflight_count}
                    </span>
                  </div>
                  <div className='h-1 w-full overflow-hidden rounded-full bg-muted'>
                    <div
                      className={cn("h-full rounded-full transition-[width]", tone.barClass)}
                      style={{ width: `${inflightPct}%` }}
                    />
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
};

function livenessTone(lastClaimAt: string | null) {
  if (!lastClaimAt) {
    return {
      label: "Stale",
      dotClass: "bg-muted-foreground/60",
      labelClass: "text-muted-foreground",
      barClass: "bg-muted-foreground/40"
    };
  }
  const ageSecs = (Date.now() - new Date(lastClaimAt).getTime()) / 1_000;
  if (ageSecs < 60) {
    return {
      label: "Active",
      dotClass: "bg-emerald-500",
      labelClass: "text-emerald-700 dark:text-emerald-400",
      barClass: "bg-emerald-500"
    };
  }
  if (ageSecs < 300) {
    return {
      label: "Idle",
      dotClass: "bg-amber-500",
      labelClass: "text-amber-700 dark:text-amber-400",
      barClass: "bg-amber-500"
    };
  }
  return {
    label: "Stale",
    dotClass: "bg-destructive",
    labelClass: "text-destructive",
    barClass: "bg-destructive"
  };
}

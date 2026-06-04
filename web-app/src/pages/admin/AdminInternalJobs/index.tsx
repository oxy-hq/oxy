import { useQueryClient } from "@tanstack/react-query";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useQueueStats } from "@/hooks/api/internalJobs";
import queryKeys from "@/hooks/api/queryKey";
import type { QueueStatusCounts } from "@/services/api/internalJobs";
import { JobInspector } from "./components/JobInspector";
import { KpiStrip } from "./components/KpiStrip";
import { LiveIndicator } from "./components/LiveIndicator";
import { ScheduledJobsPanel } from "./components/ScheduledJobsPanel";
import { ThroughputChart } from "./components/ThroughputChart";
import { WorkerFleetPanel } from "./components/WorkerFleetPanel";
import { LiveProvider, useLive } from "./LiveContext";
import { useInternalJobsHistory } from "./useInternalJobsHistory";

/**
 * Internal Jobs operator console.
 *
 * **NOT** the customer-facing Coordinator / Orchestrator UI. This page
 * is for Oxy staff to answer ops questions: "is the worker fleet
 * healthy?", "did the dead-letter queue grow overnight?", "when did
 * the reaper last run?".
 *
 * The 2026-06 overhaul reshapes the page around the patterns shared
 * by Sidekiq / Hangfire / BullMQ Board:
 *
 *   - **Live header** — pause/refresh + "Updated 3s ago" pill in the
 *     top bar.
 *   - **KPI strip with sparklines + delta arrows** — six tiles for
 *     queued / claimed / completed / failed / cancelled / dead. Each
 *     carries a 5-min sparkline and a delta-since-page-open.
 *   - **Throughput chart** — multi-series SVG of completed vs failed
 *     deltas between 5s polls. Hover for per-tick tooltip.
 *   - **Worker fleet grid** — one card per `worker_id`, color-coded
 *     by claim recency, with inflight bar.
 *   - **Scheduled jobs** — periodic system loops with manual-trigger
 *     buttons.
 *   - **Tabbed job inspector** — failure-shape grouping + bulk
 *     select + sticky retry/delete bar.
 */
export default function AdminInternalJobsPage() {
  return (
    <LiveProvider>
      <PageBody />
    </LiveProvider>
  );
}

function PageBody() {
  const { paused, togglePaused } = useLive();
  const qc = useQueryClient();
  const queueStats = useQueueStats({ paused });
  const history = useInternalJobsHistory(queueStats.data, queueStats.dataUpdatedAt);

  // Manual refresh — invalidate everything so all panels re-fetch
  // at once, not just the queue-stats hook. Matches the operator's
  // mental model: "show me the current state, all of it".
  const onRefresh = () => {
    qc.invalidateQueries({ queryKey: queryKeys.internalJobs.all });
  };

  return (
    <div className='mx-auto max-w-7xl space-y-8 p-6 lg:px-10 lg:py-10'>
      <header className='flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between'>
        <div className='space-y-2'>
          <p className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
            Admin · Operations
          </p>
          <h1 className='font-semibold text-3xl tracking-tight'>Internal jobs</h1>
          <p className='max-w-2xl text-muted-foreground text-sm'>
            Worker fleet health, queue throughput, and dead-letter triage for the system meta queue.
            Numbers refresh every 5 seconds while live.
          </p>
        </div>
        <LiveIndicator
          updatedAt={queueStats.dataUpdatedAt || undefined}
          paused={paused}
          onTogglePaused={togglePaused}
          onRefresh={onRefresh}
          isFetching={queueStats.isFetching}
        />
      </header>

      <KpiSection
        data={queueStats.data}
        isLoading={queueStats.isLoading}
        isError={queueStats.isError}
        history={history}
      />

      <ThroughputChart history={history} />

      <div className='grid gap-6 lg:grid-cols-2'>
        <WorkerFleetPanel />
        <ScheduledJobsPanel />
      </div>

      <JobInspector />
    </div>
  );
}

const KpiSection = ({
  data,
  isLoading,
  isError,
  history
}: {
  data:
    | { total: QueueStatusCounts; last_1h: QueueStatusCounts; last_24h: QueueStatusCounts }
    | undefined;
  isLoading: boolean;
  isError: boolean;
  history: ReturnType<typeof useInternalJobsHistory>;
}) => {
  if (isLoading) return <Skeleton className='h-32 w-full' />;
  if (isError || !data) {
    return (
      <div className='rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-destructive text-sm'>
        Failed to load queue stats.
      </div>
    );
  }
  return (
    <KpiStrip
      total={data.total}
      last_1h={data.last_1h}
      last_24h={data.last_24h}
      history={history}
    />
  );
};

import { useQueryClient } from "@tanstack/react-query";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useQueueStats } from "@/hooks/api/internalJobs";
import queryKeys from "@/hooks/api/queryKey";
import { HealthRibbon } from "./components/HealthRibbon";
import { JobsConsole } from "./components/JobsConsole";
import { LiveIndicator } from "./components/LiveIndicator";
import { ScheduledJobsPanel } from "./components/ScheduledJobsPanel";
import { WorkerFleetPanel } from "./components/WorkerFleetPanel";
import { LiveProvider, useLive } from "./LiveContext";
import { useInternalJobsHistory } from "./useInternalJobsHistory";

/**
 * Internal Jobs operator console — Oxy-staff cockpit for the agentic task
 * queue and worker fleet. NOT the customer-facing Orchestrator UI.
 *
 * The 2026-06 cockpit pass inverts the old emphasis: realtime charts are
 * demoted to a single compact health ribbon, and the detailed jobs console
 * (failed/dead jobs, each drillable into a full debug panel with the
 * workspace / org / user / error / decoded spec) becomes the centerpiece —
 * because "which job broke, whose was it, and why" matters more here than a
 * live graph.
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

  const onRefresh = () => {
    qc.invalidateQueries({ queryKey: queryKeys.internalJobs.all });
  };

  return (
    <div className='mx-auto max-w-7xl space-y-5 p-6 lg:px-10 lg:py-8'>
      <header className='flex items-center justify-between gap-4'>
        <div className='flex items-baseline gap-3'>
          <p className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.18em]'>
            Operations
          </p>
          <span className='text-muted-foreground/40'>/</span>
          <h1 className='font-semibold text-xl tracking-tight'>Internal jobs</h1>
        </div>
        <LiveIndicator
          updatedAt={queueStats.dataUpdatedAt || undefined}
          paused={paused}
          onTogglePaused={togglePaused}
          onRefresh={onRefresh}
          isFetching={queueStats.isFetching}
        />
      </header>

      {queueStats.isLoading ? (
        <Skeleton className='h-16 w-full' />
      ) : queueStats.isError || !queueStats.data ? (
        <div className='rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-destructive text-xs'>
          Failed to load queue stats.
        </div>
      ) : (
        <HealthRibbon total={queueStats.data.total} history={history} />
      )}

      <JobsConsole />

      <div className='grid gap-5 lg:grid-cols-2'>
        <WorkerFleetPanel />
        <ScheduledJobsPanel />
      </div>
    </div>
  );
}

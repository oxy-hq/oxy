import { CheckCircle2 } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useRecentFailures } from "@/hooks/api/internalJobs";
import { useLive } from "../../LiveContext";
import { relativeTime } from "../../utils";

const LIMIT = 100;

/**
 * Recent-failures inspector. Read-only view of the most recent
 * `failed | dead` rows from the queue, with `task_type` as the
 * primary pivot. Operators come here when they're answering "what
 * just went wrong?" — the dead-letter tab is the same data filtered
 * down to terminal state.
 *
 * The empty state ("No failed or dead tasks. Healthy.") is intended
 * to be reassuring — green check, calm copy.
 */
export const RecentFailuresTab = () => {
  const { paused } = useLive();
  const { data, isLoading, isError } = useRecentFailures(LIMIT, { paused });

  if (isLoading) return <Skeleton className='h-40 w-full' />;
  if (isError || !data) {
    return (
      <div className='rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-destructive text-sm'>
        Failed to load recent failures.
      </div>
    );
  }
  if (data.length === 0) {
    return (
      <div className='flex flex-col items-center justify-center gap-2 rounded-lg border border-border/60 border-dashed bg-muted/20 px-6 py-12 text-center'>
        <CheckCircle2 className='size-6 text-emerald-600' />
        <p className='font-medium text-sm'>No failed or dead tasks.</p>
        <p className='text-muted-foreground text-xs'>The worker fleet is healthy.</p>
      </div>
    );
  }

  return (
    <div className='overflow-hidden rounded-lg border border-border/60 bg-card'>
      <div className='grid grid-cols-[1fr_auto_auto_auto_auto] gap-3 border-border/60 border-b px-3 py-2 font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
        <span>Task</span>
        <span>Status</span>
        <span>Worker</span>
        <span>Claims</span>
        <span>Updated</span>
      </div>
      <div className='divide-y divide-border/60'>
        {data.map((row) => (
          <div
            key={row.task_id}
            className='grid grid-cols-[1fr_auto_auto_auto_auto] items-center gap-3 px-3 py-2 text-xs transition-colors hover:bg-muted/20'
          >
            <div className='flex min-w-0 items-center gap-2'>
              <span className='truncate font-mono text-muted-foreground'>{row.task_id}</span>
              <span className='shrink-0 rounded-full bg-muted/60 px-1.5 py-0.5 font-mono text-[10px]'>
                {row.task_type ?? "?"}
              </span>
            </div>
            <Badge
              variant={row.queue_status === "dead" ? "destructive" : "outline"}
              className='text-[10px]'
            >
              {row.queue_status}
            </Badge>
            <span className='max-w-[18ch] shrink-0 truncate font-mono text-[10px] text-muted-foreground'>
              {row.worker_id ?? "—"}
            </span>
            <span className='shrink-0 text-muted-foreground tabular-nums'>
              {row.claim_count}/{row.max_claims}
            </span>
            <span className='shrink-0 text-muted-foreground tabular-nums'>
              {relativeTime(row.updated_at)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
};

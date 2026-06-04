import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useQueueStats } from "@/hooks/api/internalJobs";
import type { QueueStatusCounts } from "@/services/api/internalJobs";
import { SectionCard } from "./SectionCard";

const STATUSES: { key: keyof QueueStatusCounts; label: string; tone: string }[] = [
  { key: "queued", label: "Queued", tone: "text-foreground" },
  { key: "claimed", label: "Claimed", tone: "text-primary" },
  { key: "completed", label: "Completed", tone: "text-foreground" },
  { key: "failed", label: "Failed", tone: "text-destructive" },
  { key: "cancelled", label: "Cancelled", tone: "text-muted-foreground" },
  { key: "dead", label: "Dead", tone: "text-destructive" }
];

export function QueueStatsSection() {
  const { data, isLoading, isError } = useQueueStats();

  return (
    <SectionCard
      title='Queue stats'
      description='Counts by queue_status, bucketed by row updated_at.'
    >
      {isLoading ? (
        <Skeleton className='h-24 w-full' />
      ) : isError || !data ? (
        <p className='text-destructive text-sm'>Failed to load queue stats.</p>
      ) : (
        <div className='grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-6'>
          {STATUSES.map(({ key, label, tone }) => (
            <div key={key} className='rounded-md border border-border bg-background p-3'>
              <div className='text-muted-foreground text-xs uppercase tracking-wide'>{label}</div>
              <div className={`mt-1 font-semibold text-2xl ${tone}`}>{data.total[key]}</div>
              <div className='mt-2 grid grid-cols-2 gap-1 text-muted-foreground text-xs'>
                <span>1h: {data.last_1h[key]}</span>
                <span>24h: {data.last_24h[key]}</span>
              </div>
            </div>
          ))}
        </div>
      )}
    </SectionCard>
  );
}

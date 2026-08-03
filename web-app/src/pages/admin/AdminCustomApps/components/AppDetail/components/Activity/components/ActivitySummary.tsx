import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useAppActivitySummary } from "@/hooks/api/customApps/useCustomApps";
import { relativeTime } from "../relativeTime";
import { ActivityStat } from "./ActivityStat";

/** The "is anyone using this?" answer at a glance. */
export const ActivitySummary = ({ appId }: { appId: string }) => {
  const { data, isLoading } = useAppActivitySummary(appId);

  if (isLoading) {
    return (
      <div className='grid @2xl:grid-cols-4 grid-cols-2 gap-3'>
        {Array.from({ length: 4 }).map((_, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: static skeleton
          <Skeleton key={i} className='h-20 w-full' />
        ))}
      </div>
    );
  }

  return (
    // Stat columns key off the dossier panel's width, not the viewport's — the
    // panel can be 400px wide on a 1440px screen, where four columns is soup.
    <div
      className='grid @2xl:grid-cols-4 grid-cols-2 gap-3'
      data-testid='admin-app-activity-summary'
    >
      <ActivityStat
        id='last-viewed'
        label='Last viewed'
        value={data?.last_viewed_at ? relativeTime(data.last_viewed_at) : "never"}
      />
      <ActivityStat
        id='unique-users'
        label='Unique users (7d)'
        value={(data?.unique_users_7d ?? 0).toString()}
      />
      <ActivityStat
        id='total-views'
        label='Total views (7d)'
        value={(data?.total_views_7d ?? 0).toString()}
      />
      <ActivityStat
        id='total-events'
        label='Total events (7d)'
        value={(data?.total_events_7d ?? 0).toString()}
      />
    </div>
  );
};

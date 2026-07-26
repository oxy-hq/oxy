import { Activity as ActivityIcon, Users } from "lucide-react";
import { useState } from "react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import {
  useAppActivityEventGroups,
  useAppActivityEventOccurrences,
  useAppActivitySummary,
  useAppActivityVisitors
} from "@/hooks/api/customApps/useCustomApps";

/**
 * Activity tab. Shows three sections in vertical stack:
 *
 *   1. Headline card — last viewed / 7d uniques / 7d total views /
 *      7d total events. The "is anyone using this?" answer at a glance.
 *   2. Visitors table — per-user roll-up over the last 7 days
 *      (sessions, views, first seen, last seen) sorted by recency.
 *   3. Events table — engineer-tagged custom events grouped by name,
 *      with click-to-drill-down into recent occurrences.
 *
 * Storage is PostgreSQL; queries are app-admin gated. See
 * `custom_apps_activity` on the backend for the SQL.
 */
export const Activity = ({ appId }: { appId: string }) => (
  <div className='space-y-4 p-4 pt-0'>
    <SummaryCard appId={appId} />
    <VisitorsSection appId={appId} />
    <EventsSection appId={appId} />
  </div>
);

const SummaryCard = ({ appId }: { appId: string }) => {
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
    <div className='grid @2xl:grid-cols-4 grid-cols-2 gap-3'>
      <Stat
        label='Last viewed'
        value={data?.last_viewed_at ? relativeTime(data.last_viewed_at) : "never"}
      />
      <Stat label='Unique users (7d)' value={(data?.unique_users_7d ?? 0).toString()} />
      <Stat label='Total views (7d)' value={(data?.total_views_7d ?? 0).toString()} />
      <Stat label='Total events (7d)' value={(data?.total_events_7d ?? 0).toString()} />
    </div>
  );
};

const Stat = ({ label, value }: { label: string; value: string }) => (
  <div className='rounded-md border bg-card p-3'>
    <div className='text-muted-foreground text-xs uppercase tracking-wider'>{label}</div>
    <div className='mt-1 font-medium text-lg'>{value}</div>
  </div>
);

const VisitorsSection = ({ appId }: { appId: string }) => {
  const { data, isLoading } = useAppActivityVisitors(appId, 7);

  return (
    <section>
      <h3 className='mb-2 flex items-center gap-1.5 font-medium text-muted-foreground text-xs uppercase tracking-wider'>
        <Users className='size-3.5' />
        Visitors (last 7 days)
      </h3>
      {isLoading ? (
        <Skeleton className='h-32 w-full' />
      ) : (data?.length ?? 0) === 0 ? (
        <p className='text-muted-foreground text-sm'>
          No views yet. Custom apps record a view when a user opens the app's HTML — bundle assets /
          API fetches are excluded so this counts page loads, not request volume.
        </p>
      ) : (
        <div className='overflow-hidden rounded-md border'>
          <table className='w-full text-sm'>
            <thead className='bg-muted/40 text-muted-foreground text-xs uppercase tracking-wider'>
              <tr>
                <th className='px-3 py-2 text-left font-medium'>Visitor</th>
                <th className='px-3 py-2 text-right font-medium'>Sessions</th>
                <th className='px-3 py-2 text-right font-medium'>Views</th>
                <th className='px-3 py-2 text-right font-medium'>First seen</th>
                <th className='px-3 py-2 text-right font-medium'>Last seen</th>
              </tr>
            </thead>
            <tbody>
              {data?.map((v) => (
                <tr key={v.user_id} className='border-t'>
                  <td className='px-3 py-2 font-mono text-xs'>{v.user_email}</td>
                  <td className='px-3 py-2 text-right'>{v.sessions}</td>
                  <td className='px-3 py-2 text-right'>{v.views}</td>
                  <td className='px-3 py-2 text-right text-muted-foreground text-xs'>
                    {relativeTime(v.first_seen_at)}
                  </td>
                  <td className='px-3 py-2 text-right text-muted-foreground text-xs'>
                    {relativeTime(v.last_seen_at)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
};

const EventsSection = ({ appId }: { appId: string }) => {
  const [drillName, setDrillName] = useState<string | null>(null);
  const groups = useAppActivityEventGroups(appId, 7);
  const occurrences = useAppActivityEventOccurrences(appId, drillName, 7);

  return (
    <section>
      <h3 className='mb-2 flex items-center gap-1.5 font-medium text-muted-foreground text-xs uppercase tracking-wider'>
        <ActivityIcon className='size-3.5' />
        Events (last 7 days)
      </h3>
      {groups.isLoading ? (
        <Skeleton className='h-24 w-full' />
      ) : (groups.data?.length ?? 0) === 0 ? (
        <p className='text-muted-foreground text-sm'>
          No custom events recorded yet. Engineers call{" "}
          <code>useTrackEvent(&quot;name&quot;, payload)</code> from the bundle to log interactions
          (button clicks, filter changes, etc.); they surface here grouped by name.
        </p>
      ) : (
        <div className='overflow-hidden rounded-md border'>
          <table className='w-full text-sm'>
            <thead className='bg-muted/40 text-muted-foreground text-xs uppercase tracking-wider'>
              <tr>
                <th className='px-3 py-2 text-left font-medium'>Event</th>
                <th className='px-3 py-2 text-right font-medium'>Count</th>
                <th className='px-3 py-2 text-right font-medium'>Last fired</th>
              </tr>
            </thead>
            <tbody>
              {groups.data?.map((g) => (
                <tr
                  key={g.event_name}
                  className='cursor-pointer border-t hover:bg-accent/40'
                  onClick={() => setDrillName(drillName === g.event_name ? null : g.event_name)}
                >
                  <td className='px-3 py-2 font-mono text-xs'>{g.event_name}</td>
                  <td className='px-3 py-2 text-right'>{g.count}</td>
                  <td className='px-3 py-2 text-right text-muted-foreground text-xs'>
                    {relativeTime(g.last_fired_at)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {drillName && (
        <div className='mt-3 rounded-md border bg-muted/20 p-3'>
          <div className='mb-2 flex items-center justify-between'>
            <span className='font-mono text-xs'>{drillName} — recent occurrences</span>
            <button
              type='button'
              className='text-muted-foreground text-xs hover:text-foreground'
              onClick={() => setDrillName(null)}
            >
              Close
            </button>
          </div>
          {occurrences.isLoading ? (
            <Skeleton className='h-32 w-full' />
          ) : (occurrences.data?.length ?? 0) === 0 ? (
            <p className='text-muted-foreground text-sm'>No occurrences in the window.</p>
          ) : (
            <div className='space-y-2'>
              {occurrences.data?.map((o) => (
                <div key={o.id} className='rounded border bg-background p-2'>
                  <div className='flex items-center justify-between font-mono text-muted-foreground text-xs'>
                    <span>{o.user_email}</span>
                    <span>{relativeTime(o.occurred_at)}</span>
                  </div>
                  <pre className='mt-1 overflow-auto whitespace-pre-wrap font-mono text-xs'>
                    {JSON.stringify(o.payload, null, 2)}
                  </pre>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </section>
  );
};

function relativeTime(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return iso;
  const diff = Date.now() - then;
  const m = Math.floor(diff / 60_000);
  if (m < 1) return "just now";
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  if (d < 30) return `${d}d ago`;
  return new Date(iso).toLocaleDateString();
}

import { Users } from "lucide-react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useAppActivityVisitors } from "@/hooks/api/customApps/useCustomApps";
import { relativeTime } from "../relativeTime";

/** Per-user roll-up over the last 7 days, sorted by recency. */
export const ActivityVisitors = ({ appId }: { appId: string }) => {
  const { data, isLoading } = useAppActivityVisitors(appId, 7);

  return (
    <section data-testid='admin-app-activity-visitors'>
      <h3 className='mb-2 flex items-center gap-1.5 font-medium text-muted-foreground text-xs uppercase tracking-wider'>
        <Users className='size-3.5' />
        Visitors (last 7 days)
      </h3>
      {isLoading ? (
        <Skeleton className='h-32 w-full' />
      ) : (data?.length ?? 0) === 0 ? (
        <p
          className='text-muted-foreground text-xs'
          data-testid='admin-app-activity-visitors-empty'
        >
          No views yet. Custom apps record a view when a user opens the app's HTML — bundle assets /
          API fetches are excluded so this counts page loads, not request volume.
        </p>
      ) : (
        <div className='overflow-hidden rounded-md border'>
          <table className='w-full text-xs'>
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
                <tr key={v.user_id} className='border-t' data-testid='admin-app-activity-visitor'>
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

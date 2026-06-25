import { ThreadRow } from "@/components/ThreadRow";
import useThreads from "@/hooks/api/threads/useThreads";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";

const RECENT_LIMIT = 5;

interface RecentThreadRow {
  id: string;
  title: string;
  timestamp: string;
  to: string;
}

/**
 * The workspace's latest threads (newest-first, capped at 5) — the calm
 * "Recent threads" footnote on the HQ launcher. For the searchable, paginated
 * list used by the chat page and the Ask dock, see `ThreadHistory`.
 */
export function RecentThreads({ className }: { className?: string }) {
  const { project } = useCurrentProjectBranch();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const ws = ROUTES.ORG(orgSlug).WORKSPACE(project.id);

  const threadsQuery = useThreads({ page: 1, limit: RECENT_LIMIT });
  // Render nothing while loading (avoid layout shift on the launcher).
  if (threadsQuery.isLoading) return null;

  const rows: RecentThreadRow[] = (threadsQuery.data?.threads ?? [])
    .map((t) => ({
      id: t.id,
      title: t.title || t.input || "(untitled thread)",
      timestamp: t.created_at,
      to: ws.THREAD(t.id)
    }))
    .sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime())
    .slice(0, RECENT_LIMIT);

  if (rows.length === 0) return null;

  return (
    <div className={className}>
      <p className='mb-1.5 font-medium text-muted-foreground/70 text-xs uppercase tracking-wide'>
        Recent threads
      </p>
      <div data-testid='recent-threads-list'>
        {rows.map((row) => (
          <ThreadRow key={row.id} title={row.title} timestamp={row.timestamp} to={row.to} />
        ))}
      </div>
    </div>
  );
}

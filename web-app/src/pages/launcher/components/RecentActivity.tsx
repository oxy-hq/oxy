import { MessagesSquare, Workflow } from "lucide-react";
import { Link } from "react-router-dom";
import useRunHistory from "@/hooks/api/coordinator/useRunHistory";
import useThreads from "@/hooks/api/threads/useThreads";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { timeAgo } from "@/libs/utils/date";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";

const RECENT_LIMIT = 5;

interface RecentItem {
  id: string;
  title: string;
  timestamp: string;
  kind: "thread" | "run";
  status?: string;
  to: string;
}

/**
 * A compact "Recent" footnote on the launcher: merges the latest threads and
 * workflow/procedure (automation) runs into a single newest-first list capped
 * at 5 rows. Pipeline (airway), agent, and system runs are excluded.
 */
export function RecentActivity() {
  const { project } = useCurrentProjectBranch();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const ws = ROUTES.ORG(orgSlug).WORKSPACE(project.id);

  const threadsQuery = useThreads(1, RECENT_LIMIT);
  // source_type="workflow" narrows server-side to DAG/procedure runs only,
  // excluding airway (elt), agent, preagg_cycle, and other system runs.
  const runsQuery = useRunHistory({ limit: RECENT_LIMIT, offset: 0, source_type: "workflow" });

  // While both queries are loading, render nothing (avoid layout shift).
  if (threadsQuery.isLoading && runsQuery.isPending) return null;

  const threadItems: RecentItem[] = (threadsQuery.data?.threads ?? []).map((t) => ({
    id: t.id,
    title: t.title || t.input || "(untitled thread)",
    timestamp: t.created_at,
    kind: "thread",
    to: ws.THREAD(t.id)
  }));

  const runItems: RecentItem[] = (runsQuery.data?.runs ?? []).map((r) => ({
    id: r.run_id,
    title: r.question || "(untitled run)",
    timestamp: r.created_at,
    kind: "run",
    status: r.status,
    to: ws.IDE.COORDINATOR.RUN_DETAIL(r.run_id)
  }));

  const merged = [...threadItems, ...runItems]
    .sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime())
    .slice(0, RECENT_LIMIT);

  if (merged.length === 0) return null;

  return (
    <div className='mx-auto w-full max-w-6xl px-6 pb-8'>
      <p className='mb-1.5 font-medium text-muted-foreground/70 text-xs uppercase tracking-wide'>
        Recent activity
      </p>
      <div data-testid='recent-activity-list'>
        {merged.map((item) => (
          <Link
            key={`${item.kind}:${item.id}`}
            to={item.to}
            className='flex items-center gap-2 rounded px-1 py-1.5 text-muted-foreground text-xs hover:text-foreground'
          >
            {item.kind === "thread" ? (
              <MessagesSquare className='size-3.5 shrink-0' />
            ) : (
              <Workflow className='size-3.5 shrink-0' />
            )}
            <span className='min-w-0 flex-1 truncate'>{item.title}</span>
            <div className='flex shrink-0 items-center gap-2'>
              {item.kind === "run" && item.status === "failed" && (
                <span className='text-destructive text-xs'>failed</span>
              )}
              <span className='text-muted-foreground/60 text-xs'>{timeAgo(item.timestamp)}</span>
            </div>
          </Link>
        ))}
      </div>
    </div>
  );
}

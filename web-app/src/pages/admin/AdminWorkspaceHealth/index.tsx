import { useQueryClient } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { Link } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import queryKeys from "@/hooks/api/queryKey";
import { useWorkspaceHealth } from "@/hooks/api/workspaceHealth/useWorkspaceHealth";
import { timeAgo } from "@/libs/utils/date";
import ROUTES from "@/libs/utils/routes";
import { AdminStatusPill } from "@/pages/admin/components/AdminStatusPill";
import { workspaceHealthTone } from "@/pages/admin/components/workspaceHealthTone";

/**
 * Cross-tenant workspace health rollup. Fetches from
 * `GET /admin/workspace-health` and renders a worst-first table with
 * a status badge and per-workspace reason list. Each row deep-links into
 * the workspace detail page's Health tab for single-workspace drill-in.
 */

export default function AdminWorkspaceHealthPage() {
  const qc = useQueryClient();
  const { data, isLoading, isError, isFetching } = useWorkspaceHealth();

  const onRefresh = () => {
    qc.invalidateQueries({ queryKey: queryKeys.workspaceHealth.all });
  };

  return (
    <div
      className='mx-auto max-w-7xl space-y-5 p-6 lg:px-10 lg:py-8'
      data-testid='admin-workspace-health-page'
    >
      <header className='flex items-center justify-between gap-4'>
        <div className='flex items-baseline gap-3'>
          <p className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.18em]'>
            Operations
          </p>
          <span className='text-muted-foreground/40'>/</span>
          <h1 className='font-semibold text-xl tracking-tight'>Workspace health</h1>
        </div>
        <Button
          variant='outline'
          size='sm'
          onClick={onRefresh}
          disabled={isFetching}
          className='gap-1.5'
        >
          <RefreshCw className={isFetching ? "animate-spin" : ""} aria-hidden />
          Refresh
        </Button>
      </header>

      {isLoading ? (
        <div className='space-y-2'>
          <Skeleton className='h-10 w-full' />
          <Skeleton className='h-10 w-full' />
          <Skeleton className='h-10 w-full' />
        </div>
      ) : isError || !data ? (
        <div className='rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-destructive text-sm'>
          Failed to load workspace health data.
        </div>
      ) : data.workspaces.length === 0 ? (
        <div className='rounded-lg border border-border bg-muted/30 p-6 text-center text-muted-foreground text-sm'>
          No workspaces found.
        </div>
      ) : (
        <div className='overflow-hidden rounded-lg border border-border'>
          <table className='w-full text-sm'>
            <thead>
              <tr className='border-border border-b bg-muted/30'>
                <th className='px-4 py-2.5 text-left font-medium text-muted-foreground text-xs uppercase tracking-wide'>
                  Workspace
                </th>
                <th className='px-4 py-2.5 text-left font-medium text-muted-foreground text-xs uppercase tracking-wide'>
                  Status
                </th>
                <th className='px-4 py-2.5 text-left font-medium text-muted-foreground text-xs uppercase tracking-wide'>
                  Reasons
                </th>
                <th className='px-4 py-2.5 text-right font-medium text-muted-foreground text-xs uppercase tracking-wide'>
                  Last checked
                </th>
              </tr>
            </thead>
            <tbody>
              {data.workspaces.map((ws) => (
                <tr
                  key={ws.workspace_id}
                  className='border-border border-b last:border-0 hover:bg-muted/20'
                  data-testid='workspace-health-row'
                  data-status={ws.status}
                >
                  <td className='px-4 py-3'>
                    <Link
                      to={`${ROUTES.ADMIN.WORKSPACE_DETAIL(ws.workspace_id)}?tab=health`}
                      className='group block'
                    >
                      <span className='font-medium text-sm group-hover:underline'>
                        {ws.workspace_name ?? "Unknown workspace"}
                      </span>
                      <span className='block text-muted-foreground text-xs'>
                        {ws.org_name ? `${ws.org_name} · ` : ""}
                        <span className='font-mono'>{ws.workspace_id}</span>
                      </span>
                    </Link>
                  </td>
                  <td className='px-4 py-3'>
                    <AdminStatusPill
                      tone={workspaceHealthTone(ws.status)}
                      label={ws.status}
                      data-testid='workspace-health-status-badge'
                    />
                  </td>
                  <td className='px-4 py-3'>
                    {ws.reasons.length === 0 ? (
                      <span className='text-muted-foreground/60'>—</span>
                    ) : (
                      <ul className='list-none space-y-0.5'>
                        {ws.reasons.map((reason) => (
                          <li key={reason} className='text-muted-foreground text-xs'>
                            {reason}
                          </li>
                        ))}
                      </ul>
                    )}
                  </td>
                  <td className='px-4 py-3 text-right text-muted-foreground/70 text-xs'>
                    {ws.checked_at ? (
                      <span title={new Date(ws.checked_at).toLocaleString()}>
                        {timeAgo(ws.checked_at)}
                      </span>
                    ) : (
                      <span className='text-muted-foreground/50'>—</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {data && (
        <p className='text-muted-foreground/60 text-xs'>
          {data.workspaces.length} workspace{data.workspaces.length !== 1 ? "s" : ""} — sorted
          worst-first
        </p>
      )}
    </div>
  );
}

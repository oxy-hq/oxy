import { useQueryClient } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { Link } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { Table, TableBody, TableCell, TableHeader, TableRow } from "@/components/ui/shadcn/table";
import queryKeys from "@/hooks/api/queryKey";
import { useWorkspaceHealth } from "@/hooks/api/workspaceHealth/useWorkspaceHealth";
import { timeAgo } from "@/libs/utils/date";
import ROUTES from "@/libs/utils/routes";
import { AdminStatusPill } from "@/pages/admin/components/AdminStatusPill";
import {
  ADMIN_HEADER_ROW_CLASS,
  ADMIN_ROW_CLASS,
  AdminTh
} from "@/pages/admin/components/AdminTable";
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
        <div className='rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-destructive text-xs'>
          Failed to load workspace health data.
        </div>
      ) : data.workspaces.length === 0 ? (
        <div className='rounded-lg border border-border bg-muted/30 p-6 text-center text-muted-foreground text-xs'>
          No workspaces found.
        </div>
      ) : (
        <div className='overflow-hidden rounded-lg border border-border/60'>
          <Table>
            <TableHeader>
              <TableRow className={ADMIN_HEADER_ROW_CLASS}>
                <AdminTh>Workspace</AdminTh>
                <AdminTh>Status</AdminTh>
                <AdminTh>Reasons</AdminTh>
                <AdminTh align='right'>Last checked</AdminTh>
              </TableRow>
            </TableHeader>
            <TableBody>
              {data.workspaces.map((ws) => (
                <TableRow
                  key={ws.workspace_id}
                  className={ADMIN_ROW_CLASS}
                  data-testid='workspace-health-row'
                  data-status={ws.status}
                >
                  <TableCell>
                    <Link
                      to={`${ROUTES.ADMIN.WORKSPACE_DETAIL(ws.workspace_id)}?tab=health`}
                      className='group block'
                    >
                      <span className='font-medium text-xs group-hover:underline'>
                        {ws.workspace_name ?? "Unknown workspace"}
                      </span>
                      <span className='block text-muted-foreground text-xs'>
                        {ws.org_name ? `${ws.org_name} · ` : ""}
                        <span className='font-mono text-[10px]'>{ws.workspace_id}</span>
                      </span>
                    </Link>
                  </TableCell>
                  <TableCell>
                    <AdminStatusPill
                      tone={workspaceHealthTone(ws.status)}
                      label={ws.status}
                      data-testid='workspace-health-status-badge'
                    />
                  </TableCell>
                  <TableCell>
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
                  </TableCell>
                  <TableCell className='text-right text-muted-foreground text-xs tabular-nums'>
                    {ws.checked_at ? (
                      <span title={new Date(ws.checked_at).toLocaleString()}>
                        {timeAgo(ws.checked_at)}
                      </span>
                    ) : (
                      <span className='text-muted-foreground/50'>—</span>
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
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

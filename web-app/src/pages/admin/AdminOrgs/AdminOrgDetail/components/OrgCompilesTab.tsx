import { FileCheck } from "lucide-react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import { useCompiles } from "@/hooks/api/compiles";
import { ago } from "@/pages/admin/AdminExplorer/format";
import { AdminEmptyState } from "../../../components/AdminEmptyState";
import { AdminStatusPill } from "../../../components/AdminStatusPill";

const STATUS_TONE: Record<string, "ok" | "warn" | "danger" | "muted"> = {
  ready: "ok",
  compiling: "warn",
  failed: "danger"
};

/**
 * Compile health for one org: every revision across the org's workspaces
 * (server-side `org_id` filter), newest first, with a status rollup. Polls on
 * the shared 5s cadence so an in-flight compile's row ticks. `workspaceNames`
 * resolves the revision's workspace id to a human label (compile rows don't
 * carry the name).
 */
export const OrgCompilesTab = ({
  orgId,
  workspaceNames
}: {
  orgId: string;
  workspaceNames: Record<string, string>;
}) => {
  const { data, isPending, isError } = useCompiles({ org_id: orgId, limit: 100 });
  const rows = data?.rows ?? [];
  const ready = rows.filter((r) => r.status === "ready").length;
  const failed = rows.filter((r) => r.status === "failed").length;
  const compiling = rows.filter((r) => r.status === "compiling").length;

  if (isPending) {
    return <Skeleton className='h-64 w-full' />;
  }
  if (isError) {
    return (
      <div className='rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-destructive text-sm'>
        Failed to load compiles.
      </div>
    );
  }
  if (rows.length === 0) {
    return (
      <AdminEmptyState
        icon={FileCheck}
        title='No compiles yet'
        description="Revisions appear here once this org's workspaces are compiled."
      />
    );
  }

  return (
    <div className='space-y-4'>
      <div className='flex flex-wrap items-center gap-2'>
        <AdminStatusPill tone='ok' label={`${ready} ready`} />
        {compiling > 0 ? <AdminStatusPill tone='warn' label={`${compiling} compiling`} /> : null}
        {failed > 0 ? <AdminStatusPill tone='danger' label={`${failed} failed`} /> : null}
        <span className='ml-auto text-muted-foreground text-xs tabular-nums'>
          {rows.length}
          {rows.length >= 100 ? "+" : ""} revision{rows.length === 1 ? "" : "s"}
          {rows.length >= 100 ? " · latest 100" : ""}
        </span>
      </div>

      <div className='overflow-hidden rounded-lg border border-border/60 bg-card'>
        <Table>
          <TableHeader>
            <TableRow className='hover:bg-transparent'>
              <TableHead className='text-[10px] uppercase tracking-[0.14em]'>
                Workspace · Branch
              </TableHead>
              <TableHead className='text-[10px] uppercase tracking-[0.14em]'>Status</TableHead>
              <TableHead className='text-[10px] uppercase tracking-[0.14em]'>Files</TableHead>
              <TableHead className='text-right text-[10px] uppercase tracking-[0.14em]'>
                Started
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map((r) => (
              <TableRow key={r.revision_id} className='text-xs'>
                <TableCell className='max-w-0'>
                  <div className='flex min-w-0 items-center gap-2'>
                    <span className='truncate font-medium'>
                      {workspaceNames[r.workspace_id] ?? r.workspace_id.slice(0, 8)}
                    </span>
                    {r.branch ? (
                      <span className='shrink-0 font-mono text-[10px] text-muted-foreground'>
                        {r.branch}
                      </span>
                    ) : null}
                    {r.is_current_for_workspace ? (
                      <span className='shrink-0 rounded-full bg-muted/60 px-1.5 py-0.5 text-[9px] uppercase'>
                        current
                      </span>
                    ) : null}
                  </div>
                </TableCell>
                <TableCell>
                  <AdminStatusPill tone={STATUS_TONE[r.status] ?? "muted"} label={r.status} />
                </TableCell>
                <TableCell className='text-muted-foreground tabular-nums'>
                  {r.file_count_compiled}/{r.file_count_seen}
                  {r.file_count_failed > 0 ? (
                    <span className='text-destructive'> · {r.file_count_failed} failed</span>
                  ) : null}
                </TableCell>
                <TableCell className='text-right text-muted-foreground tabular-nums'>
                  {ago(r.started_at)}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
    </div>
  );
};

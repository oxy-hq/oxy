import { useQueryClient } from "@tanstack/react-query";
import { Loader2, PlayCircle, RefreshCw } from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useCompiles, useRunCompileNow } from "@/hooks/api/compiles";
import queryKeys from "@/hooks/api/queryKey";
import type { CompileRow } from "@/services/api/compiles";
import { LiveIndicator } from "../AdminInternalJobs/components/LiveIndicator";

/**
 * `/admin/compiles` — operator view of the compile-boundary
 * `revisions` table. Mirrors the live-console pattern of
 * `/admin/internal-jobs`:
 *
 *   - 5-second polling with a LiveIndicator pause/refresh
 *   - One row per revision (newest first)
 *   - Status pill colour matches the FailureKind taxonomy
 *   - "Current" badge when the workspaces row points at this
 *     revision (i.e. promotion happened)
 *   - "Run compile now" form: enqueue a `Compile` TaskSpec for a
 *     given workspace, optional promote
 *
 * Why this page exists: GitHub webhooks fire compiles automatically
 * in cloud mode, but operators need to (a) see whether they're
 * actually running, (b) inspect failed files, (c) trigger an ad-hoc
 * compile when investigating an incident. Everything else (semantic
 * compiles, preagg cycles) lives under Internal jobs already.
 */
export default function AdminCompilesPage() {
  const [paused, setPaused] = useState(false);
  const [workspaceFilter, setWorkspaceFilter] = useState<string>("");
  const [statusFilter, setStatusFilter] = useState<string>("");

  const qc = useQueryClient();
  const params = useMemo(
    () => ({
      limit: 50,
      workspace_id: workspaceFilter.trim() ? workspaceFilter.trim() : undefined,
      status: statusFilter || undefined
    }),
    [workspaceFilter, statusFilter]
  );
  const compiles = useCompiles(params, { paused });

  // Defensive defaults — `data` is undefined during initial load, and
  // a misconfigured deployment (DB unreachable, migration not yet
  // applied) can return a response where individual fields are
  // missing. Collapsing both to safe primitives at the top means the
  // render tree below never sees `undefined`.
  const rows = compiles.data?.rows ?? [];
  const totalReturned = compiles.data?.total_returned ?? rows.length;

  const onRefresh = () => {
    qc.invalidateQueries({ queryKey: queryKeys.compiles.all });
  };

  return (
    <div className='mx-auto max-w-7xl space-y-6 p-6 lg:px-10 lg:py-8'>
      <header className='flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between'>
        <div className='space-y-1'>
          <p className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
            Admin · Compile boundary
          </p>
          <h1 className='font-semibold text-2xl tracking-tight'>Compile revisions</h1>
          <p className='max-w-2xl text-muted-foreground text-sm'>
            Each row is one compile attempt against a workspace. GitHub pushes auto-enqueue these
            with <code className='font-mono'>promote=true</code>; the manual form below queues
            ad-hoc compiles for investigation.
          </p>
        </div>
        <LiveIndicator
          updatedAt={compiles.dataUpdatedAt || undefined}
          paused={paused}
          onTogglePaused={() => setPaused((p) => !p)}
          onRefresh={onRefresh}
          isFetching={compiles.isFetching}
        />
      </header>

      <RunCompileForm />

      <section className='space-y-3'>
        <div className='flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between'>
          <div className='flex gap-2'>
            <div className='space-y-1'>
              <label
                htmlFor='compiles-workspace-filter'
                className='block font-medium text-[10px] text-muted-foreground uppercase tracking-wider'
              >
                Workspace ID
              </label>
              <Input
                id='compiles-workspace-filter'
                placeholder='Filter by workspace UUID'
                value={workspaceFilter}
                onChange={(e) => setWorkspaceFilter(e.target.value)}
                className='h-8 w-72 font-mono text-xs'
              />
            </div>
            <div className='space-y-1'>
              <label
                htmlFor='compiles-status-filter'
                className='block font-medium text-[10px] text-muted-foreground uppercase tracking-wider'
              >
                Status
              </label>
              <select
                id='compiles-status-filter'
                value={statusFilter}
                onChange={(e) => setStatusFilter(e.target.value)}
                className='h-8 rounded-md border border-input bg-background px-2 text-xs'
              >
                <option value=''>any</option>
                <option value='compiling'>compiling</option>
                <option value='ready'>ready</option>
                <option value='failed'>failed</option>
              </select>
            </div>
          </div>
          <span className='font-medium text-[11px] text-muted-foreground tabular-nums'>
            {totalReturned} recent
          </span>
        </div>

        {compiles.isLoading ? (
          <Skeleton className='h-40 w-full' />
        ) : compiles.isError ? (
          <div className='rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-destructive text-sm'>
            Failed to load compile revisions.
          </div>
        ) : rows.length === 0 ? (
          <div className='rounded-lg border border-border/60 border-dashed bg-muted/30 p-6 text-center text-muted-foreground text-sm'>
            No revisions yet. The first compile will appear here once it runs.
          </div>
        ) : (
          <CompilesTable rows={rows} />
        )}
      </section>
    </div>
  );
}

const RunCompileForm = () => {
  const [workspaceId, setWorkspaceId] = useState("");
  const [gitSha, setGitSha] = useState("");
  const [branch, setBranch] = useState("");
  const [promote, setPromote] = useState(false);
  const mutation = useRunCompileNow();

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = workspaceId.trim();
    if (!trimmed) {
      toast.error("Workspace ID is required");
      return;
    }
    mutation.mutate(
      {
        workspace_id: trimmed,
        git_sha: gitSha.trim() || undefined,
        branch: branch.trim() || undefined,
        promote
      },
      {
        onSuccess: (res) => {
          toast.success(`Enqueued compile task ${res.task_id.slice(0, 8)}…`);
          setGitSha("");
          setBranch("");
        },
        onError: (err) => {
          toast.error(err instanceof Error ? err.message : "Enqueue failed");
        }
      }
    );
  };

  return (
    <form onSubmit={submit} className='space-y-3 rounded-lg border border-border/60 bg-card p-4'>
      <div className='flex items-center justify-between'>
        <h3 className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
          Run compile now
        </h3>
        <PlayCircle className='size-4 text-muted-foreground' aria-hidden />
      </div>
      <div className='grid grid-cols-1 gap-3 md:grid-cols-4'>
        <div className='space-y-1 md:col-span-2'>
          <label
            htmlFor='run-workspace-id'
            className='block font-medium text-[10px] text-muted-foreground uppercase tracking-wider'
          >
            Workspace ID *
          </label>
          <Input
            id='run-workspace-id'
            placeholder='00000000-0000-0000-0000-000000000000'
            value={workspaceId}
            onChange={(e) => setWorkspaceId(e.target.value)}
            className='h-8 font-mono text-xs'
            required
          />
        </div>
        <div className='space-y-1'>
          <label
            htmlFor='run-git-sha'
            className='block font-medium text-[10px] text-muted-foreground uppercase tracking-wider'
          >
            Git SHA
          </label>
          <Input
            id='run-git-sha'
            placeholder='auto'
            value={gitSha}
            onChange={(e) => setGitSha(e.target.value)}
            className='h-8 font-mono text-xs'
          />
        </div>
        <div className='space-y-1'>
          <label
            htmlFor='run-branch'
            className='block font-medium text-[10px] text-muted-foreground uppercase tracking-wider'
          >
            Branch
          </label>
          <Input
            id='run-branch'
            placeholder='main'
            value={branch}
            onChange={(e) => setBranch(e.target.value)}
            className='h-8 text-xs'
          />
        </div>
      </div>
      <div className='flex items-center justify-between'>
        <label className='inline-flex cursor-pointer items-center gap-2 text-xs'>
          <input
            type='checkbox'
            checked={promote}
            onChange={(e) => setPromote(e.target.checked)}
            className='size-3.5'
          />
          Promote the resulting revision into{" "}
          <code className='font-mono'>workspaces.current_revision_id</code>
        </label>
        <Button type='submit' size='sm' disabled={mutation.isPending} className='h-8'>
          {mutation.isPending ? (
            <Loader2 className='mr-1.5 size-3 animate-spin' />
          ) : (
            <RefreshCw className='mr-1.5 size-3' />
          )}
          Enqueue
        </Button>
      </div>
    </form>
  );
};

const CompilesTable = ({ rows }: { rows: CompileRow[] }) => (
  <div className='overflow-hidden rounded-lg border border-border/60 bg-card'>
    <table className='w-full text-xs'>
      <thead className='bg-muted/40 text-[10px] text-muted-foreground uppercase tracking-wider'>
        <tr className='border-border/60 border-b'>
          <th className='px-3 py-2 text-left font-medium'>Status</th>
          <th className='px-3 py-2 text-left font-medium'>Workspace</th>
          <th className='px-3 py-2 text-left font-medium'>Git</th>
          <th className='px-3 py-2 text-left font-medium'>Files</th>
          <th className='px-3 py-2 text-left font-medium'>Duration</th>
          <th className='px-3 py-2 text-left font-medium'>Started</th>
          <th className='px-3 py-2 text-left font-medium'>Compiler</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => (
          <CompileRowItem key={row.revision_id} row={row} />
        ))}
      </tbody>
    </table>
  </div>
);

const CompileRowItem = ({ row }: { row: CompileRow }) => (
  <tr className='border-border/40 border-b hover:bg-muted/20'>
    <td className='px-3 py-2'>
      <div className='flex items-center gap-2'>
        <StatusBadge status={row.status} />
        {row.is_current_for_workspace ? (
          <Badge className='border-emerald-500/40 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'>
            current
          </Badge>
        ) : null}
        {row.kind === "draft" ? (
          <Badge variant='outline' className='text-[10px]'>
            draft
          </Badge>
        ) : null}
      </div>
    </td>
    <td className='px-3 py-2 font-mono text-[11px]'>{safeSlice(row.workspace_id, 8)}…</td>
    <td className='px-3 py-2'>
      <div className='flex flex-col leading-tight'>
        <span className='font-mono text-[11px]'>{safeSlice(row.git_sha, 12)}</span>
        {row.branch ? (
          <span className='text-[10px] text-muted-foreground'>{row.branch}</span>
        ) : null}
      </div>
    </td>
    <td className='px-3 py-2 tabular-nums'>
      <span className='font-medium'>{row.file_count_compiled}</span>
      <span className='text-muted-foreground'> / {row.file_count_seen}</span>
      {row.file_count_failed > 0 ? (
        <span className='ml-1 text-destructive'>({row.file_count_failed} failed)</span>
      ) : null}
    </td>
    <td className='px-3 py-2 tabular-nums'>
      {row.duration_ms !== null ? formatMs(row.duration_ms) : "—"}
    </td>
    <td className='px-3 py-2 text-muted-foreground tabular-nums'>
      {formatRelative(row.started_at)}
    </td>
    <td className='px-3 py-2 font-mono text-[10px] text-muted-foreground'>
      {row.compiler_version}
    </td>
  </tr>
);

const StatusBadge = ({ status }: { status: string }) => {
  const cls = (() => {
    switch (status) {
      case "ready":
        return "border-emerald-500/40 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300";
      case "failed":
        return "border-destructive/40 bg-destructive/10 text-destructive";
      case "compiling":
        return "border-amber-500/40 bg-amber-500/10 text-amber-700 dark:text-amber-300";
      default:
        return "";
    }
  })();
  return <Badge className={cls}>{status}</Badge>;
};

/** Slice a possibly-undefined string. Lets the table render without
 *  crashing on a malformed row from a misconfigured deployment.
 */
function safeSlice(value: string | null | undefined, end: number): string {
  if (typeof value !== "string") return "—";
  return value.slice(0, end);
}

function formatMs(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const m = Math.floor(ms / 60_000);
  const s = Math.floor((ms % 60_000) / 1000);
  return `${m}m ${s}s`;
}

function formatRelative(iso: string): string {
  const then = new Date(iso).getTime();
  const now = Date.now();
  const diff = Math.max(0, Math.floor((now - then) / 1000));
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

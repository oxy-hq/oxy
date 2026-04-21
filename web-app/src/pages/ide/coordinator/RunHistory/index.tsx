import { ChevronLeft, ChevronRight, Circle, RefreshCw, Square, XCircle } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import useRunHistory from "@/hooks/api/coordinator/useRunHistory";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";
import type { RunHistoryEntry } from "@/services/api/coordinator";
import useCurrentOrg from "@/stores/useCurrentOrg";

// ── Constants ───────────────────────────────────────────────────────────────

const PAGE_SIZE = 25;

const STATUS_OPTIONS = [
  { value: "", label: "All statuses" },
  { value: "running", label: "Running" },
  { value: "suspended", label: "Suspended" },
  { value: "done", label: "Done" },
  { value: "failed", label: "Failed" },
  { value: "cancelled", label: "Cancelled" }
];

const SOURCE_OPTIONS = [
  { value: "", label: "All sources" },
  { value: "analytics", label: "Analytics" },
  { value: "builder", label: "Builder" }
];

// ── Status badge ────────────────────────────────────────────────────────────

const statusConfig: Record<string, { label: string; className: string; icon: React.ElementType }> =
  {
    running: { label: "Running", className: "text-primary", icon: RefreshCw },
    suspended: { label: "Suspended", className: "text-warning", icon: Circle },
    done: { label: "Done", className: "text-emerald-500", icon: Circle },
    failed: { label: "Failed", className: "text-destructive", icon: XCircle },
    cancelled: { label: "Cancelled", className: "text-muted-foreground", icon: Square }
  };

const StatusBadge: React.FC<{ status: string }> = ({ status }) => {
  const config = statusConfig[status] ?? statusConfig.running;
  const Icon = config.icon;
  return (
    <span className={cn("inline-flex items-center gap-1 font-medium text-xs", config.className)}>
      <Icon className={cn("h-3 w-3", status === "running" && "animate-spin")} />
      {config.label}
    </span>
  );
};

// ── Run row ─────────────────────────────────────────────────────────────────

const RunRow: React.FC<{ run: RunHistoryEntry; projectId: string }> = ({ run, projectId }) => {
  const navigate = useNavigate();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";

  const handleClick = () => {
    navigate(ROUTES.ORG(orgSlug).WORKSPACE(projectId).IDE.COORDINATOR.RUN_TREE(run.run_id));
  };

  const date = new Date(run.created_at);
  const dateStr = date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
  const timeStr = date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });

  return (
    <div
      role='button'
      tabIndex={0}
      className='flex cursor-pointer items-center gap-4 border-border border-b px-4 py-2.5 last:border-b-0 hover:bg-muted/50'
      onClick={handleClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") handleClick();
      }}
    >
      <div className='w-24 shrink-0'>
        <StatusBadge status={run.status} />
      </div>
      <div className='min-w-0 flex-1'>
        <p className='truncate text-sm'>{run.question}</p>
        <div className='mt-0.5 flex items-center gap-2'>
          <span className='text-muted-foreground text-xs'>{run.agent_id || run.source_type}</span>
          {run.attempt > 0 && (
            <span className='text-warning text-xs'>attempt {run.attempt + 1}</span>
          )}
        </div>
      </div>
      <div className='w-32 shrink-0 text-right'>
        <span className='text-muted-foreground text-xs'>
          {dateStr} {timeStr}
        </span>
        <div className='font-mono text-muted-foreground text-xs'>{run.run_id.slice(0, 8)}</div>
      </div>
    </div>
  );
};

// ── Page ────────────────────────────────────────────────────────────────────

const RunHistoryPage: React.FC = () => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  const [page, setPage] = useState(0);
  const [statusFilter, setStatusFilter] = useState("");
  const [sourceFilter, setSourceFilter] = useState("");

  const { data, isPending, error, refetch } = useRunHistory({
    limit: PAGE_SIZE,
    offset: page * PAGE_SIZE,
    status: statusFilter || undefined,
    source_type: sourceFilter || undefined
  });

  const runs = data?.runs ?? [];
  const total = data?.total ?? 0;
  const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE));

  const handleFilterChange = () => {
    setPage(0);
  };

  if (isPending && page === 0) {
    return (
      <div className='flex h-full items-center justify-center'>
        <Spinner className='h-6 w-6' />
      </div>
    );
  }

  if (error) {
    return (
      <div className='flex h-full flex-col items-center justify-center gap-2'>
        <p className='text-destructive text-sm'>Failed to load run history</p>
        <Button variant='outline' size='sm' onClick={() => refetch()}>
          Retry
        </Button>
      </div>
    );
  }

  return (
    <div className='flex h-full flex-col'>
      {/* Header */}
      <div className='flex items-center justify-between border-border border-b px-4 py-3'>
        <div>
          <h2 className='font-semibold text-base'>Run History</h2>
          <p className='text-muted-foreground text-xs'>
            {total} {total === 1 ? "run" : "runs"} total
          </p>
        </div>
        <Button variant='ghost' size='icon' onClick={() => refetch()} className='h-8 w-8'>
          <RefreshCw className='h-4 w-4' />
        </Button>
      </div>

      {/* Filters */}
      <div className='flex items-center gap-3 border-border border-b px-4 py-2'>
        <select
          value={statusFilter}
          onChange={(e) => {
            setStatusFilter(e.target.value);
            handleFilterChange();
          }}
          className='h-8 rounded-md border border-border bg-background px-2 text-sm'
        >
          {STATUS_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {opt.label}
            </option>
          ))}
        </select>
        <select
          value={sourceFilter}
          onChange={(e) => {
            setSourceFilter(e.target.value);
            handleFilterChange();
          }}
          className='h-8 rounded-md border border-border bg-background px-2 text-sm'
        >
          {SOURCE_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {opt.label}
            </option>
          ))}
        </select>
      </div>

      {/* Table body */}
      <div className='flex-1 overflow-y-auto'>
        {isPending ? (
          <div className='flex h-32 items-center justify-center'>
            <Spinner className='h-5 w-5' />
          </div>
        ) : runs.length === 0 ? (
          <div className='flex h-full flex-col items-center justify-center gap-1 text-muted-foreground'>
            <p className='text-sm'>No runs match the current filters</p>
          </div>
        ) : (
          runs.map((run) => <RunRow key={run.run_id} run={run} projectId={projectId} />)
        )}
      </div>

      {/* Pagination */}
      {totalPages > 1 && (
        <div className='flex items-center justify-between border-border border-t px-4 py-2'>
          <span className='text-muted-foreground text-xs'>
            Page {page + 1} of {totalPages}
          </span>
          <div className='flex items-center gap-1'>
            <Button
              variant='ghost'
              size='icon'
              className='h-7 w-7'
              disabled={page === 0}
              onClick={() => setPage((p) => Math.max(0, p - 1))}
            >
              <ChevronLeft className='h-4 w-4' />
            </Button>
            <Button
              variant='ghost'
              size='icon'
              className='h-7 w-7'
              disabled={page >= totalPages - 1}
              onClick={() => setPage((p) => p + 1)}
            >
              <ChevronRight className='h-4 w-4' />
            </Button>
          </div>
        </div>
      )}
    </div>
  );
};

export default RunHistoryPage;

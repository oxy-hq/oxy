import { Circle, RefreshCw, Square, XCircle } from "lucide-react";
import type React from "react";
import { useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import useActiveRuns from "@/hooks/api/coordinator/useActiveRuns";
import useCoordinatorLive from "@/hooks/api/coordinator/useCoordinatorLive";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";
import { AnalyticsService } from "@/services/api/analytics";
import type { ActiveRunEntry } from "@/services/api/coordinator";
import useCurrentOrg from "@/stores/useCurrentOrg";

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
    <span className={cn("inline-flex items-center gap-1.5 font-medium text-xs", config.className)}>
      <Icon className={cn("h-3 w-3", status === "running" && "animate-spin")} />
      {config.label}
    </span>
  );
};

const ElapsedTime: React.FC<{ createdAt: string }> = ({ createdAt }) => {
  const elapsed = useMemo(() => {
    const start = new Date(createdAt).getTime();
    const now = Date.now();
    const secs = Math.floor((now - start) / 1000);
    if (secs < 60) return `${secs}s`;
    const mins = Math.floor(secs / 60);
    if (mins < 60) return `${mins}m ${secs % 60}s`;
    const hours = Math.floor(mins / 60);
    return `${hours}h ${mins % 60}m`;
  }, [createdAt]);

  return <span className='text-muted-foreground text-xs'>{elapsed}</span>;
};

const RunRow: React.FC<{ run: ActiveRunEntry; projectId: string; onCancel: () => void }> = ({
  run,
  projectId,
  onCancel
}) => {
  const navigate = useNavigate();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";

  const handleCancel = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await AnalyticsService.cancelRun(projectId, run.run_id);
      onCancel();
      toast.success("Run cancelled");
    } catch {
      toast.error("Failed to cancel run");
    }
  };

  const handleClick = () => {
    navigate(ROUTES.ORG(orgSlug).WORKSPACE(projectId).IDE.COORDINATOR.RUN_TREE(run.run_id));
  };

  return (
    <div
      role='button'
      tabIndex={0}
      className='flex cursor-pointer items-center gap-4 border-border border-b px-4 py-3 last:border-b-0 hover:bg-muted/50'
      onClick={handleClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") handleClick();
      }}
    >
      <div className='w-28 shrink-0'>
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
      <div className='shrink-0 text-right'>
        <ElapsedTime createdAt={run.created_at} />
        <div className='mt-0.5 font-mono text-muted-foreground text-xs'>
          {run.run_id.slice(0, 8)}
        </div>
      </div>
      <div className='w-20 shrink-0 text-right'>
        {(run.status === "running" || run.status === "suspended") && (
          <Button
            variant='ghost'
            size='sm'
            onClick={(e) => handleCancel(e)}
            className='h-7 text-xs'
          >
            Cancel
          </Button>
        )}
      </div>
    </div>
  );
};

const ActiveRunsPage: React.FC = () => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const { data, isPending, error, refetch } = useActiveRuns();

  // Subscribe to live SSE updates for real-time invalidation.
  useCoordinatorLive();

  if (isPending) {
    return (
      <div className='flex h-full items-center justify-center'>
        <Spinner className='h-6 w-6' />
      </div>
    );
  }

  if (error) {
    return (
      <div className='flex h-full flex-col items-center justify-center gap-2'>
        <p className='text-destructive text-sm'>Failed to load active runs</p>
        <Button variant='outline' size='sm' onClick={() => refetch()}>
          Retry
        </Button>
      </div>
    );
  }

  const runs = data?.runs ?? [];

  return (
    <div className='flex h-full flex-col'>
      <div className='flex items-center justify-between border-border border-b px-4 py-3'>
        <div>
          <h2 className='font-semibold text-base'>Active Runs</h2>
          <p className='text-muted-foreground text-xs'>
            {runs.length} active {runs.length === 1 ? "run" : "runs"}
          </p>
        </div>
        <Button variant='ghost' size='icon' onClick={() => refetch()} className='h-8 w-8'>
          <RefreshCw className='h-4 w-4' />
        </Button>
      </div>

      <div className='flex-1 overflow-y-auto'>
        {runs.length === 0 ? (
          <div className='flex h-full flex-col items-center justify-center gap-1 text-muted-foreground'>
            <Circle className='h-8 w-8 opacity-40' />
            <p className='text-sm'>No active runs</p>
            <p className='text-xs'>Runs will appear here when a pipeline is executing</p>
          </div>
        ) : (
          runs.map((run) => (
            <RunRow key={run.run_id} run={run} projectId={projectId} onCancel={() => refetch()} />
          ))
        )}
      </div>
    </div>
  );
};

export default ActiveRunsPage;

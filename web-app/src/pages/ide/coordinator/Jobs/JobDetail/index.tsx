import { ArrowLeft, Pencil, Play } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { useSchedules } from "@/hooks/api/schedules/useSchedules";
import { useRole } from "@/hooks/useRole";
import { cn } from "@/libs/shadcn/utils";
import { isSystemSchedule, targetKindToJobType } from "../../components/constants";
import { JobTypeBadge } from "../../components/JobTypeBadge";
import { EmptyState, ErrorState, LoadingState } from "../../components/PageState";
import { SystemBadge } from "../../components/SystemBadge";
import { useCoordinatorRoutes } from "../../components/useCoordinatorRoutes";
import RunNowDialog from "../components/RunNowDialog";
import ScheduleDialog from "../components/ScheduleDialog";
import { JobDefinitionCard } from "./components/JobDefinitionCard";
import { JobHealthCard } from "./components/JobHealthCard";
import { JobRunsCard } from "./components/JobRunsCard";

/** Job detail — one job over time, with its definition, health, and actions. */
const JobDetailPage: React.FC = () => {
  const { scheduleId } = useParams<{ scheduleId: string }>();
  const { data, isPending, error, refetch } = useSchedules();
  const routes = useCoordinatorRoutes();
  const canManage = useRole().is.workspaceAdmin;

  const [editOpen, setEditOpen] = useState(false);
  const [runOpen, setRunOpen] = useState(false);

  if (isPending) return <LoadingState />;
  if (error) return <ErrorState message='Failed to load job' onRetry={refetch} />;

  const schedule = data?.find((s) => s.id === scheduleId);
  if (!schedule) {
    return (
      <EmptyState
        title='Job not found'
        hint='This job may have been deleted.'
        action={
          <Button asChild size='sm' variant='outline'>
            <Link to={routes.JOBS}>Back to jobs</Link>
          </Button>
        }
      />
    );
  }

  const jobType = targetKindToJobType(schedule.target_kind);
  const isSystem = isSystemSchedule(schedule);

  return (
    <div className='flex h-full flex-col'>
      <div className='flex items-center gap-3 border-border border-b px-4 py-2.5'>
        <Button asChild variant='ghost' size='icon' className='h-8 w-8'>
          <Link to={routes.JOBS} aria-label='Back to jobs'>
            <ArrowLeft className='h-4 w-4' />
          </Link>
        </Button>
        {isSystem ? <SystemBadge variant='icon' /> : <JobTypeBadge type={jobType} variant='icon' />}
        <div className='min-w-0'>
          <h2 className='truncate font-semibold text-base leading-tight'>{schedule.name}</h2>
          <p className='font-mono text-muted-foreground text-xs'>{schedule.target_ref}</p>
        </div>
        <span
          className={cn(
            "ml-2 rounded-full px-2 py-0.5 font-medium text-xs",
            schedule.enabled ? "bg-success/10 text-success" : "bg-muted text-muted-foreground"
          )}
        >
          {schedule.enabled ? "Enabled" : "Paused"}
        </span>
        {canManage && (
          <div className='ml-auto flex items-center gap-2'>
            <Button
              size='sm'
              variant='outline'
              data-testid='coordinator-run-now-button'
              onClick={() => setRunOpen(true)}
            >
              <Play className='h-4 w-4' />
              Run now
            </Button>
            <Button
              size='sm'
              variant='outline'
              data-testid='coordinator-edit-button'
              onClick={() => setEditOpen(true)}
            >
              <Pencil className='h-4 w-4' />
              Edit
            </Button>
          </div>
        )}
      </div>

      <div className='flex-1 overflow-y-auto'>
        <div className='grid grid-cols-1 gap-4 p-4 lg:grid-cols-2'>
          <JobDefinitionCard schedule={schedule} />
          <JobHealthCard schedule={schedule} canManage={canManage} />
          <JobRunsCard scheduleId={schedule.id} />
        </div>
      </div>

      <ScheduleDialog open={editOpen} onOpenChange={setEditOpen} schedule={schedule} />
      <RunNowDialog open={runOpen} onOpenChange={setRunOpen} schedule={schedule} />
    </div>
  );
};

export default JobDetailPage;

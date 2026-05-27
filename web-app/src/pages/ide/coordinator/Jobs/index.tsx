import { CalendarClock, Plus, RefreshCw } from "lucide-react";
import type React from "react";
import { useMemo, useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import { useSchedules } from "@/hooks/api/schedules/useSchedules";
import { useRole } from "@/hooks/useRole";
import { targetKindToJobType } from "../components/constants";
import { ErrorState, LoadingState } from "../components/PageState";
import { JobRow } from "./components/JobRow";
import { DEFAULT_JOB_FILTERS, type JobFilters, JobsFilterBar } from "./components/JobsFilterBar";
import ScheduleDialog from "./components/ScheduleDialog";

/**
 * Jobs — the catalog of job definitions. Answers "what's scheduled, and is
 * it on?". Job type is a filter, not a tab: one unified table, filtered down.
 */
const JobsPage: React.FC = () => {
  const { data, isPending, error, refetch } = useSchedules();
  // `useRole().is.workspaceAdmin` reads the workspace-level role, which is
  // populated in local mode too (org role is null when no org context).
  const canManage = useRole().is.workspaceAdmin;

  const [filters, setFilters] = useState<JobFilters>(DEFAULT_JOB_FILTERS);
  const [createOpen, setCreateOpen] = useState(false);

  const jobs = useMemo(() => {
    const q = filters.search.trim().toLowerCase();
    return (data ?? []).filter((s) => {
      if (filters.type !== "all" && targetKindToJobType(s.target_kind) !== filters.type)
        return false;
      if (filters.state === "enabled" && !s.enabled) return false;
      if (filters.state === "disabled" && s.enabled) return false;
      if (filters.health === "error" && !s.last_error) return false;
      if (filters.health === "healthy" && s.last_error) return false;
      if (q && !s.name.toLowerCase().includes(q) && !s.target_ref.toLowerCase().includes(q))
        return false;
      return true;
    });
  }, [data, filters]);

  return (
    <div className='flex h-full flex-col'>
      <div className='flex items-center justify-between border-border border-b px-4 py-2.5'>
        <div>
          <h2 className='font-semibold text-base'>Jobs</h2>
          <p className='text-muted-foreground text-xs'>
            {data?.length ?? 0} {data?.length === 1 ? "job" : "jobs"} defined
          </p>
        </div>
        <div className='flex items-center gap-2'>
          <Button
            variant='ghost'
            size='icon'
            onClick={() => refetch()}
            className='h-8 w-8'
            tooltip={{ content: "Refresh" }}
          >
            <RefreshCw className='h-4 w-4' />
          </Button>
          {canManage && (
            <Button
              size='sm'
              data-testid='coordinator-new-job-button'
              onClick={() => setCreateOpen(true)}
            >
              <Plus className='h-4 w-4' />
              New job
            </Button>
          )}
        </div>
      </div>

      <div className='border-border border-b px-4 py-2'>
        <JobsFilterBar value={filters} onChange={setFilters} />
      </div>

      {isPending ? (
        <LoadingState />
      ) : error ? (
        <ErrorState message='Failed to load jobs' onRetry={refetch} />
      ) : (
        <div className='flex-1 overflow-y-auto'>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Job</TableHead>
                <TableHead>Schedule</TableHead>
                <TableHead>Next run</TableHead>
                <TableHead>Last run</TableHead>
                <TableHead>On</TableHead>
                <TableHead className='w-10' />
              </TableRow>
            </TableHeader>
            <TableBody>
              {jobs.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={6}>
                    <div className='flex flex-col items-center gap-1.5 py-12 text-center text-muted-foreground'>
                      <CalendarClock className='h-8 w-8 opacity-40' />
                      <p className='font-medium text-foreground text-sm'>
                        {data && data.length > 0
                          ? "No jobs match these filters"
                          : "No jobs scheduled yet"}
                      </p>
                      <p className='text-xs'>
                        Schedule a DAG workflow or ELT pipeline to run on a recurring cron.
                      </p>
                    </div>
                  </TableCell>
                </TableRow>
              ) : (
                jobs.map((s) => <JobRow key={s.id} schedule={s} canManage={canManage} />)
              )}
            </TableBody>
          </Table>
        </div>
      )}

      <ScheduleDialog open={createOpen} onOpenChange={setCreateOpen} />
    </div>
  );
};

export default JobsPage;

import { Button } from "@/components/ui/shadcn/button";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import { useRunReaper, useScheduledJobs } from "@/hooks/api/internalJobs";
import { formatInterval, relativeTime } from "../utils";
import { SectionCard } from "./SectionCard";

export function ScheduledJobsSection() {
  const { data, isLoading, isError } = useScheduledJobs();
  const runReaper = useRunReaper();

  return (
    <SectionCard
      title='Scheduled jobs'
      description='Periodic system loops registered with the runtime. Static metadata for now; last-run timestamps are not yet wired up.'
    >
      {isLoading ? (
        <Skeleton className='h-24 w-full' />
      ) : isError || !data ? (
        <p className='text-destructive text-sm'>Failed to load scheduled jobs.</p>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead>Interval</TableHead>
              <TableHead>Description</TableHead>
              <TableHead>Last run</TableHead>
              <TableHead className='text-right'>Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {data.map((job) => (
              <TableRow key={job.name}>
                <TableCell className='font-mono text-xs'>{job.name}</TableCell>
                <TableCell className='text-sm'>{formatInterval(job.interval_secs)}</TableCell>
                <TableCell className='text-muted-foreground text-sm'>{job.description}</TableCell>
                <TableCell className='text-sm'>
                  {job.last_known_run_at ? relativeTime(job.last_known_run_at) : "—"}
                </TableCell>
                <TableCell className='text-right'>
                  {job.name === "reaper" && job.trigger_path ? (
                    <Button
                      size='sm'
                      variant='outline'
                      disabled={runReaper.isPending}
                      onClick={() => runReaper.mutate()}
                    >
                      Run now
                    </Button>
                  ) : null}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </SectionCard>
  );
}

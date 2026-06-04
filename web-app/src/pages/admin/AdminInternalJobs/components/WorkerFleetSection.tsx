import { Skeleton } from "@/components/ui/shadcn/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import { useWorkers } from "@/hooks/api/internalJobs";
import { relativeTime } from "../utils";
import { SectionCard } from "./SectionCard";

export function WorkerFleetSection() {
  const { data, isLoading, isError } = useWorkers();

  return (
    <SectionCard
      title='Worker fleet'
      description='Derived from agentic_task_queue.worker_id over the last 24h of claims.'
    >
      {isLoading ? (
        <Skeleton className='h-24 w-full' />
      ) : isError || !data ? (
        <p className='text-destructive text-sm'>Failed to load worker fleet.</p>
      ) : !data.supported ? (
        <p className='text-muted-foreground text-sm'>
          Worker fleet info isn't available for this deployment.
        </p>
      ) : data.workers.length === 0 ? (
        <p className='text-muted-foreground text-sm'>No worker activity in the last 24 hours.</p>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Worker ID</TableHead>
              <TableHead>Last claim</TableHead>
              <TableHead className='text-right'>Inflight</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {data.workers.map((w) => (
              <TableRow key={w.worker_id}>
                <TableCell className='font-mono text-xs'>{w.worker_id}</TableCell>
                <TableCell className='text-sm'>{relativeTime(w.last_claim_at)}</TableCell>
                <TableCell className='text-right tabular-nums'>{w.inflight_count}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </SectionCard>
  );
}

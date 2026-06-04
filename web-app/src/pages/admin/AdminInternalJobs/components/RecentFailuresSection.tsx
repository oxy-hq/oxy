import { Badge } from "@/components/ui/shadcn/badge";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import { useRecentFailures } from "@/hooks/api/internalJobs";
import { relativeTime } from "../utils";
import { SectionCard } from "./SectionCard";

const LIMIT = 50;

export function RecentFailuresSection() {
  const { data, isLoading, isError } = useRecentFailures(LIMIT);

  return (
    <SectionCard
      title='Recent failures'
      description={`Last ${LIMIT} rows where queue_status is failed or dead.`}
    >
      {isLoading ? (
        <Skeleton className='h-32 w-full' />
      ) : isError || !data ? (
        <p className='text-destructive text-sm'>Failed to load recent failures.</p>
      ) : data.length === 0 ? (
        <p className='text-muted-foreground text-sm'>No failed or dead tasks. Healthy.</p>
      ) : (
        <div className='overflow-x-auto'>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Task</TableHead>
                <TableHead>Type</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Worker</TableHead>
                <TableHead>Claims</TableHead>
                <TableHead>Updated</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {data.map((row) => (
                <TableRow key={row.task_id}>
                  <TableCell className='max-w-[24ch] truncate font-mono text-xs'>
                    {row.task_id}
                  </TableCell>
                  <TableCell className='text-sm'>{row.task_type ?? "?"}</TableCell>
                  <TableCell>
                    <Badge variant={row.queue_status === "dead" ? "destructive" : "outline"}>
                      {row.queue_status}
                    </Badge>
                  </TableCell>
                  <TableCell className='max-w-[18ch] truncate font-mono text-xs'>
                    {row.worker_id ?? "-"}
                  </TableCell>
                  <TableCell className='text-sm tabular-nums'>
                    {row.claim_count}/{row.max_claims}
                  </TableCell>
                  <TableCell className='text-sm'>{relativeTime(row.updated_at)}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
    </SectionCard>
  );
}

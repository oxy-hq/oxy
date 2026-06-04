import { useState } from "react";
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
import { useDeadLetter, useDeleteDead, useReenqueueDead } from "@/hooks/api/internalJobs";
import { relativeTime } from "../utils";
import { SectionCard } from "./SectionCard";

const PAGE_SIZE = 50;

export function DeadLetterSection() {
  const [offset, setOffset] = useState(0);
  const { data, isLoading, isError } = useDeadLetter(PAGE_SIZE, offset);
  const reenqueue = useReenqueueDead();
  const remove = useDeleteDead();

  const onReenqueue = (taskId: string) => {
    reenqueue.mutate(taskId);
  };

  const onDelete = (taskId: string) => {
    if (window.confirm(`Permanently delete dead task ${taskId}?`)) {
      remove.mutate(taskId);
    }
  };

  return (
    <SectionCard
      title='Dead-letter queue'
      description='Tasks that exceeded max_claims while their workers died holding them.'
    >
      {isLoading ? (
        <Skeleton className='h-32 w-full' />
      ) : isError || !data ? (
        <p className='text-destructive text-sm'>Failed to load dead-letter queue.</p>
      ) : data.rows.length === 0 ? (
        <p className='text-muted-foreground text-sm'>Dead-letter queue is empty.</p>
      ) : (
        <>
          <div className='overflow-x-auto'>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Task</TableHead>
                  <TableHead>Type</TableHead>
                  <TableHead>Claims</TableHead>
                  <TableHead>Updated</TableHead>
                  <TableHead className='text-right'>Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.rows.map((row) => (
                  <TableRow key={row.task_id}>
                    <TableCell className='max-w-[28ch] truncate font-mono text-xs'>
                      {row.task_id}
                    </TableCell>
                    <TableCell className='text-sm'>{row.task_type ?? "?"}</TableCell>
                    <TableCell className='text-sm tabular-nums'>
                      {row.claim_count}/{row.max_claims}
                    </TableCell>
                    <TableCell className='text-sm'>{relativeTime(row.updated_at)}</TableCell>
                    <TableCell className='text-right'>
                      <div className='flex justify-end gap-2'>
                        <Button
                          size='sm'
                          variant='outline'
                          disabled={reenqueue.isPending}
                          onClick={() => onReenqueue(row.task_id)}
                        >
                          Re-enqueue
                        </Button>
                        <Button
                          size='sm'
                          variant='destructive'
                          disabled={remove.isPending}
                          onClick={() => onDelete(row.task_id)}
                        >
                          Delete
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
          <div className='mt-4 flex items-center justify-between text-muted-foreground text-sm'>
            <span>
              Showing {offset + 1}–{offset + data.rows.length} of {data.total}
            </span>
            <div className='flex gap-2'>
              <Button
                size='sm'
                variant='outline'
                disabled={offset === 0}
                onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
              >
                Prev
              </Button>
              <Button
                size='sm'
                variant='outline'
                disabled={offset + data.rows.length >= data.total}
                onClick={() => setOffset(offset + PAGE_SIZE)}
              >
                Next
              </Button>
            </div>
          </div>
        </>
      )}
    </SectionCard>
  );
}

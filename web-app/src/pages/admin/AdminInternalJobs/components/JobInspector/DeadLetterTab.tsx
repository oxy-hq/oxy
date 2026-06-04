import { useQueryClient } from "@tanstack/react-query";
import { ChevronDown, ChevronRight, Inbox, Loader2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Checkbox } from "@/components/ui/shadcn/checkbox";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useDeadLetter, useDeleteDead, useReenqueueDead } from "@/hooks/api/internalJobs";
import queryKeys from "@/hooks/api/queryKey";
import { cn } from "@/libs/utils/cn";
import { InternalJobsService, type QueueRow } from "@/services/api/internalJobs";
import { useLive } from "../../LiveContext";
import { relativeTime } from "../../utils";
import { BulkActionBar } from "./BulkActionBar";
import { useFailureShapes } from "./useFailureShapes";

const PAGE_SIZE = 200;

/**
 * Dead-letter inspector. Three operator moves that the old per-row UI
 * couldn't:
 *
 *   1. Group by failure shape (task_type today, normalized error
 *      prefix when the field lands) so "97 dead = 1 bug" reads as one
 *      collapsible row rather than 97 separate ones.
 *   2. Checkbox bulk select within a group (or across groups via
 *      "select all visible").
 *   3. Sticky bottom action bar with bulk Re-enqueue / Delete, gated
 *      against an empty selection.
 *
 * Mutations are serialized client-side (Promise.all over a per-id
 * call) since the backend's reenqueue/delete endpoints are per-task.
 * A bulk endpoint is the obvious follow-up but is NOT required to
 * unblock the UI; treating each mutation independently means the
 * partial-failure UX is good (toast per failed id, the rest succeed).
 */
export const DeadLetterTab = () => {
  const { paused } = useLive();
  const qc = useQueryClient();
  const [offset, setOffset] = useState(0);
  const { data, isLoading, isError } = useDeadLetter(PAGE_SIZE, offset, { paused });
  // Per-row mutation hooks keep the existing single-row UX: success toast
  // per action, cache invalidated immediately. Bulk paths bypass these
  // hooks and call the service directly so they can emit one summary
  // toast + invalidate once after the batch settles (see onBulkReenqueue
  // / onBulkDelete below).
  const reenqueue = useReenqueueDead();
  const remove = useDeleteDead();
  const [isBulking, setIsBulking] = useState<"reenqueue" | "delete" | null>(null);
  const groups = useFailureShapes(data?.rows ?? []);

  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  // Default the largest group to expanded so the operator immediately
  // sees rows on first load without an extra click.
  useEffect(() => {
    if (groups.length > 0 && expanded.size === 0) {
      setExpanded(new Set([groups[0].key]));
    }
  }, [groups, expanded.size]);

  // Drop any selected ids that have left the current page so the bulk
  // bar count stays honest.
  useEffect(() => {
    if (!data) return;
    const live = new Set(data.rows.map((r) => r.task_id));
    setSelected((prev) => {
      const next = new Set<string>();
      prev.forEach((id) => {
        if (live.has(id)) next.add(id);
      });
      return next.size === prev.size ? prev : next;
    });
  }, [data]);

  const allVisibleIds = useMemo(() => (data?.rows ?? []).map((r) => r.task_id), [data]);

  const toggle = (taskId: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(taskId)) next.delete(taskId);
      else next.add(taskId);
      return next;
    });

  const toggleGroup = (groupRows: QueueRow[], allSelected: boolean) =>
    setSelected((prev) => {
      const next = new Set(prev);
      for (const row of groupRows) {
        if (allSelected) next.delete(row.task_id);
        else next.add(row.task_id);
      }
      return next;
    });

  const toggleExpanded = (key: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  // Bulk paths call InternalJobsService directly so the per-mutation
  // toast.success + qc.invalidateQueries from useReenqueueDead /
  // useDeleteDead don't fire N times. We collect successes / failures
  // in one settled-Promise pass and emit a single summary toast +
  // single internalJobs.all invalidation. Per-row failures still
  // surface (count in the toast) so partial failures aren't swallowed.
  const onBulkReenqueue = async () => {
    const ids = [...selected];
    if (ids.length === 0) return;
    setIsBulking("reenqueue");
    try {
      const results = await Promise.allSettled(
        ids.map((id) => InternalJobsService.reenqueueDead(id))
      );
      const ok = results.filter((r) => r.status === "fulfilled").length;
      const failed = results.length - ok;
      if (ok > 0 && failed === 0) toast.success(`Re-enqueued ${ok} task${ok === 1 ? "" : "s"}`);
      else if (ok > 0) toast.success(`Re-enqueued ${ok} / ${results.length} (${failed} failed)`);
      else toast.error(`Failed to re-enqueue ${failed} task${failed === 1 ? "" : "s"}`);
      qc.invalidateQueries({ queryKey: queryKeys.internalJobs.all });
      setSelected(new Set());
    } finally {
      setIsBulking(null);
    }
  };

  const onBulkDelete = async () => {
    const ids = [...selected];
    if (ids.length === 0) return;
    if (
      !window.confirm(`Permanently delete ${ids.length} dead task${ids.length === 1 ? "" : "s"}?`)
    )
      return;
    setIsBulking("delete");
    try {
      const results = await Promise.allSettled(ids.map((id) => InternalJobsService.deleteDead(id)));
      const ok = results.filter((r) => r.status === "fulfilled").length;
      const failed = results.length - ok;
      if (ok > 0 && failed === 0) toast.success(`Deleted ${ok} task${ok === 1 ? "" : "s"}`);
      else if (ok > 0) toast.success(`Deleted ${ok} / ${results.length} (${failed} failed)`);
      else toast.error(`Failed to delete ${failed} task${failed === 1 ? "" : "s"}`);
      qc.invalidateQueries({ queryKey: queryKeys.internalJobs.all });
      setSelected(new Set());
    } finally {
      setIsBulking(null);
    }
  };

  if (isLoading) return <Skeleton className='h-40 w-full' />;
  if (isError || !data) {
    return (
      <div className='rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-destructive text-sm'>
        Failed to load dead-letter queue.
      </div>
    );
  }
  if (data.rows.length === 0) {
    return (
      <div className='flex flex-col items-center justify-center gap-2 rounded-lg border border-border/60 border-dashed bg-muted/20 px-6 py-12 text-center'>
        <Inbox className='size-6 text-muted-foreground' />
        <p className='font-medium text-sm'>Dead-letter queue is empty.</p>
        <p className='text-muted-foreground text-xs'>
          Tasks that exhaust max_claims land here for triage. Healthy.
        </p>
      </div>
    );
  }

  const allVisibleSelected =
    allVisibleIds.length > 0 && allVisibleIds.every((id) => selected.has(id));

  return (
    <div className='space-y-3'>
      <div className='flex items-center justify-between gap-3'>
        <div className='flex items-center gap-2'>
          <Checkbox
            checked={allVisibleSelected}
            onCheckedChange={() => {
              if (allVisibleSelected) {
                setSelected(
                  (prev) => new Set([...prev].filter((id) => !allVisibleIds.includes(id)))
                );
              } else {
                setSelected((prev) => {
                  const next = new Set(prev);
                  allVisibleIds.forEach((id) => next.add(id));
                  return next;
                });
              }
            }}
            aria-label='Select all visible'
          />
          <span className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
            Select all visible
          </span>
        </div>
        <span className='font-medium text-[11px] text-muted-foreground tabular-nums'>
          {offset + 1}–{offset + data.rows.length} of {data.total.toLocaleString()}
        </span>
      </div>

      <div className='space-y-2'>
        {groups.map((group) => {
          const isExpanded = expanded.has(group.key);
          const allInGroup = group.rows.every((r) => selected.has(r.task_id));
          const someInGroup = !allInGroup && group.rows.some((r) => selected.has(r.task_id));
          return (
            <div
              key={group.key}
              className='overflow-hidden rounded-lg border border-border/60 bg-card'
            >
              <div className='flex items-center gap-3 px-3 py-2.5'>
                <Checkbox
                  checked={allInGroup ? true : someInGroup ? "indeterminate" : false}
                  onCheckedChange={() => toggleGroup(group.rows, allInGroup)}
                  aria-label={`Select all in ${group.key}`}
                />
                <button
                  type='button'
                  onClick={() => toggleExpanded(group.key)}
                  className='flex flex-1 items-center gap-2 text-left'
                >
                  {isExpanded ? (
                    <ChevronDown className='size-3.5 text-muted-foreground' />
                  ) : (
                    <ChevronRight className='size-3.5 text-muted-foreground' />
                  )}
                  <span className='font-mono text-sm'>{group.key}</span>
                  <span className='rounded-full bg-destructive/10 px-1.5 py-0.5 font-medium text-[10px] text-destructive tabular-nums'>
                    {group.rows.length} dead
                  </span>
                </button>
              </div>
              {isExpanded ? (
                <div className='divide-y divide-border/60 border-border/60 border-t'>
                  {group.rows.map((row) => {
                    const sel = selected.has(row.task_id);
                    return (
                      <div
                        key={row.task_id}
                        className={cn(
                          "flex items-center gap-3 px-3 py-2 transition-colors",
                          sel ? "bg-muted/40" : "hover:bg-muted/20"
                        )}
                      >
                        <Checkbox
                          checked={sel}
                          onCheckedChange={() => toggle(row.task_id)}
                          aria-label={`Select ${row.task_id}`}
                        />
                        <div className='flex min-w-0 flex-1 items-center gap-3 text-xs'>
                          <span className='truncate font-mono text-muted-foreground'>
                            {row.task_id}
                          </span>
                          <span className='shrink-0 rounded-full bg-muted/60 px-1.5 py-0.5 font-medium text-[10px] tabular-nums'>
                            {row.claim_count}/{row.max_claims} claims
                          </span>
                          <span className='shrink-0 text-muted-foreground tabular-nums'>
                            {relativeTime(row.updated_at)}
                          </span>
                        </div>
                        <div className='flex shrink-0 gap-1'>
                          <Button
                            size='sm'
                            variant='ghost'
                            disabled={reenqueue.isPending}
                            onClick={() => reenqueue.mutate(row.task_id)}
                            className='h-7 px-2 text-xs'
                          >
                            {reenqueue.isPending && reenqueue.variables === row.task_id ? (
                              <Loader2 className='size-3 animate-spin' />
                            ) : null}
                            Retry
                          </Button>
                          <Button
                            size='sm'
                            variant='ghost'
                            disabled={remove.isPending}
                            onClick={() => {
                              if (window.confirm(`Permanently delete dead task ${row.task_id}?`)) {
                                remove.mutate(row.task_id);
                              }
                            }}
                            className='h-7 px-2 text-destructive text-xs hover:bg-destructive/10 hover:text-destructive'
                          >
                            Delete
                          </Button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              ) : null}
            </div>
          );
        })}
      </div>

      <div className='flex items-center justify-end gap-2 pt-2'>
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

      <BulkActionBar
        selectedCount={selected.size}
        onClear={() => setSelected(new Set())}
        onReenqueue={onBulkReenqueue}
        onDelete={onBulkDelete}
        isReenqueueing={isBulking === "reenqueue"}
        isDeleting={isBulking === "delete"}
      />
    </div>
  );
};

import { Loader2, RotateCcw, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";

/**
 * Sticky bottom action bar shown when one or more dead jobs are selected in
 * the console. The caller serializes the per-id mutations and tracks progress
 * so the bar renders a single "Re-enqueueing…" state for the whole batch.
 */
export const BulkActionBar = ({
  selectedCount,
  onClear,
  onReenqueue,
  onDelete,
  isReenqueueing,
  isDeleting
}: {
  selectedCount: number;
  onClear: () => void;
  onReenqueue: () => void;
  onDelete: () => void;
  isReenqueueing?: boolean;
  isDeleting?: boolean;
}) => {
  if (selectedCount === 0) return null;
  return (
    <div className='fixed bottom-6 left-1/2 z-30 flex -translate-x-1/2 items-center gap-2 rounded-full border border-border bg-popover px-3 py-2 text-popover-foreground shadow-lg'>
      <span className='font-medium text-xs tabular-nums'>{selectedCount} selected</span>
      <Button
        variant='ghost'
        size='sm'
        onClick={onClear}
        className='h-7 gap-1 px-2 text-muted-foreground'
      >
        <X className='size-3.5' />
        Clear
      </Button>
      <Button
        variant='outline'
        size='sm'
        onClick={onReenqueue}
        disabled={isReenqueueing}
        className='h-7 gap-1.5'
      >
        {isReenqueueing ? (
          <Loader2 className='size-3.5 animate-spin' />
        ) : (
          <RotateCcw className='size-3.5' />
        )}
        Re-enqueue
      </Button>
      <Button
        variant='outline'
        size='sm'
        onClick={onDelete}
        disabled={isDeleting}
        className='h-7 gap-1.5 text-destructive hover:bg-destructive/10 hover:text-destructive'
      >
        {isDeleting ? (
          <Loader2 className='size-3.5 animate-spin' />
        ) : (
          <Trash2 className='size-3.5' />
        )}
        Delete
      </Button>
    </div>
  );
};

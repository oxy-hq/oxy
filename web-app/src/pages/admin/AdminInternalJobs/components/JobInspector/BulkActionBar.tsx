import { Loader2, RotateCcw, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";

/**
 * Sticky bottom action bar shown when one or more rows are selected
 * in the dead-letter / failures inspector. Slides up from the bottom
 * of the panel container. Modeled on the Sidekiq dead-set bulk bar:
 * the operator triages by failure shape, multi-selects within a
 * group, then bulk-retries or bulk-deletes.
 *
 * `onReenqueue` / `onDelete` receive the full set of selected ids;
 * the caller serializes the per-id mutations and tracks per-row
 * progress so the bar can render a single "Reenqueueing 5…" state.
 */
export const BulkActionBar = ({
  selectedCount,
  onClear,
  onReenqueue,
  onDelete,
  isReenqueueing,
  isDeleting,
  showReenqueue = true
}: {
  selectedCount: number;
  onClear: () => void;
  onReenqueue?: () => void;
  onDelete: () => void;
  isReenqueueing?: boolean;
  isDeleting?: boolean;
  /** Hide the re-enqueue action for read-only surfaces (e.g. failures tab). */
  showReenqueue?: boolean;
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
      {showReenqueue && onReenqueue ? (
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
      ) : null}
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

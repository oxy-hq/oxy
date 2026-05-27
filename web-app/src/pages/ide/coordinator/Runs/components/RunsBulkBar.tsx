import { Ban, RotateCcw, X } from "lucide-react";
import type React from "react";
import { Button } from "@/components/ui/shadcn/button";

/**
 * Bulk-action bar — appears only when runs are selected. Retry clones
 * terminal-failed runs via the coordinator retry endpoint; Cancel
 * signals in-flight runs to stop. The page enables each button based
 * on whether the current selection has any actionable runs of that kind.
 */
export const RunsBulkBar: React.FC<{
  count: number;
  busy: boolean;
  retryableCount: number;
  cancellableCount: number;
  onRetry: () => void;
  onCancel: () => void;
  onClear: () => void;
}> = ({ count, busy, retryableCount, cancellableCount, onRetry, onCancel, onClear }) => (
  <div className='flex items-center gap-3 border-border border-b bg-muted/40 px-4 py-2'>
    <span className='font-medium text-sm'>{count} selected</span>
    <div className='ml-auto flex items-center gap-2'>
      <Button
        size='sm'
        variant='outline'
        onClick={onRetry}
        disabled={busy || retryableCount === 0}
        tooltip={
          retryableCount === 0
            ? { content: "Only failed / cancelled / timed-out runs can be retried" }
            : undefined
        }
      >
        <RotateCcw className='h-3.5 w-3.5' />
        Retry {retryableCount > 0 ? `(${retryableCount})` : ""}
      </Button>
      <Button
        size='sm'
        variant='outline'
        onClick={onCancel}
        disabled={busy || cancellableCount === 0}
        tooltip={
          cancellableCount === 0 ? { content: "Only in-flight runs can be cancelled" } : undefined
        }
      >
        <Ban className='h-3.5 w-3.5' />
        Cancel {cancellableCount > 0 ? `(${cancellableCount})` : ""}
      </Button>
      <Button size='sm' variant='ghost' onClick={onClear}>
        <X className='h-3.5 w-3.5' />
        Clear
      </Button>
    </div>
  </div>
);

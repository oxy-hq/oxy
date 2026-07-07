import type { UseMutateFunction } from "@tanstack/react-query";
import { ArrowUpToLine, EyeOff, Loader2, Rocket, Trash2, X } from "lucide-react";
import { toast } from "sonner";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger
} from "@/components/ui/shadcn/alert-dialog";
import { Button } from "@/components/ui/shadcn/button";
import {
  useBatchDeleteApps,
  useBatchPromoteLatestApps,
  useBatchPublishApps,
  useBatchUnpublishApps
} from "@/hooks/api/customerApps/useBatchAppMutations";
import type { BatchAppResult } from "@/types/apps";

interface BulkActionBarProps {
  selectedIds: string[];
  onClear: () => void;
}

/**
 * "Published 4." / "Published 4, 1 failed — No builds to promote." Surfaces the
 * first failure's reason from the per-app results so the operator knows *why*,
 * not just how many.
 */
function summarize(res: BatchAppResult, verb: string): string {
  if (res.failed === 0) return `${verb} ${res.succeeded}.`;
  const reason = res.results.find((r) => !r.ok && r.error)?.error;
  return `${verb} ${res.succeeded}, ${res.failed} failed${reason ? ` — ${reason}` : "."}`;
}

/**
 * Sticky bulk-action bar — appears only when rows are selected. Publish and
 * Unpublish fire immediately; Delete sits behind a confirm. Each batch
 * response folds into one summary toast, and a fully-successful run clears the
 * selection (a partial failure keeps it so the operator can retry the rest).
 */
export const BulkActionBar = ({ selectedIds, onClear }: BulkActionBarProps) => {
  const promote = useBatchPromoteLatestApps();
  const publish = useBatchPublishApps();
  const unpublish = useBatchUnpublishApps();
  const del = useBatchDeleteApps();
  const count = selectedIds.length;
  if (count === 0) return null;

  const pending = promote.isPending || publish.isPending || unpublish.isPending || del.isPending;
  const label = `${count} app${count === 1 ? "" : "s"}`;

  const run = (
    mutate: UseMutateFunction<BatchAppResult, Error, string[]>,
    verb: string,
    fail: string
  ) =>
    mutate(selectedIds, {
      onSuccess: (res) => {
        if (res.failed === 0) {
          toast.success(summarize(res, verb));
          onClear();
        } else {
          toast.warning(summarize(res, verb));
        }
      },
      onError: (e) => toast.error(e instanceof Error ? e.message : fail)
    });

  return (
    <div className='fixed bottom-4 left-1/2 z-20 flex -translate-x-1/2 items-center gap-3 rounded-lg border border-border bg-card px-3 py-2 shadow-lg'>
      <span className='font-medium text-xs tabular-nums'>{label} selected</span>
      <div className='h-4 w-px bg-border' aria-hidden />

      <Button
        size='sm'
        className='h-7 gap-1.5'
        disabled={pending}
        onClick={() => run(promote.mutate, "Promoted", "Promote failed")}
        title='Point each selected app’s live channel at its newest build'
      >
        {promote.isPending ? (
          <Loader2 className='size-3.5 animate-spin' />
        ) : (
          <ArrowUpToLine className='size-3.5' />
        )}
        Promote latest
      </Button>

      <Button
        size='sm'
        variant='outline'
        className='h-7 gap-1.5'
        disabled={pending}
        onClick={() => run(publish.mutate, "Published", "Publish failed")}
      >
        {publish.isPending ? (
          <Loader2 className='size-3.5 animate-spin' />
        ) : (
          <Rocket className='size-3.5' />
        )}
        Publish
      </Button>

      <Button
        size='sm'
        variant='outline'
        className='h-7 gap-1.5'
        disabled={pending}
        onClick={() => run(unpublish.mutate, "Unpublished", "Unpublish failed")}
      >
        {unpublish.isPending ? (
          <Loader2 className='size-3.5 animate-spin' />
        ) : (
          <EyeOff className='size-3.5' />
        )}
        Unpublish
      </Button>

      <AlertDialog>
        <AlertDialogTrigger asChild>
          <Button size='sm' variant='destructive' className='h-7 gap-1.5' disabled={pending}>
            {del.isPending ? (
              <Loader2 className='size-3.5 animate-spin' />
            ) : (
              <Trash2 className='size-3.5' />
            )}
            Delete
          </Button>
        </AlertDialogTrigger>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete {label}?</AlertDialogTitle>
            <AlertDialogDescription>
              This removes the app registration and its stored bundle for each selected app. The
              customer's workspace loses access immediately. This can't be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => run(del.mutate, "Deleted", "Delete failed")}>
              Delete {label}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <Button size='sm' variant='ghost' onClick={onClear} className='h-7 gap-1 px-2 text-xs'>
        <X className='size-3.5' />
        Clear
      </Button>
    </div>
  );
};

import { useEffect, useMemo, useRef } from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useRecentCommitsInfinite } from "@/hooks/api/workspaces/useRecentCommitsInfinite";
import { useResetCommit } from "../hooks/useResetCommit";
import { CommitRow } from "./CommitRow";
import { RestoreConfirmDialog } from "./RestoreConfirmDialog";

interface Props {
  open: boolean;
  onOpenChange: (next: boolean) => void;
  workspaceId?: string;
  branch?: string;
  onResetSuccess?: () => Promise<void> | void;
}

export function HistoryDialog({ open, onOpenChange, workspaceId, branch, onResetSuccess }: Props) {
  const query = useRecentCommitsInfinite({ workspaceId, branch, enabled: open });
  const { data, fetchNextPage, hasNextPage, isFetchingNextPage, isLoading } = query;

  const reset = useResetCommit({
    workspaceId,
    branch,
    onSuccess: async () => {
      onOpenChange(false);
      await onResetSuccess?.();
    }
  });

  const commits = useMemo(() => data?.pages.flatMap((p) => p.commits) ?? [], [data]);

  const sentinelRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const node = sentinelRef.current;
    if (!node || !open) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting && hasNextPage && !isFetchingNextPage) {
          void fetchNextPage();
        }
      },
      { threshold: 0.1 }
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [open, hasNextPage, isFetchingNextPage, fetchNextPage]);

  return (
    <>
      {reset.pendingReset && (
        <RestoreConfirmDialog
          open
          shortHash={reset.pendingReset.shortHash}
          dirty={reset.pendingReset.dirty}
          discardedCommits={reset.pendingReset.discardedCommits}
          loading={reset.resettingHash === reset.pendingReset.hash}
          onConfirm={reset.confirmReset}
          onCancel={reset.cancelReset}
        />
      )}
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className='max-w-xl p-0'>
          <DialogHeader className='border-b px-4 py-3'>
            <DialogTitle className='text-sm'>Commit history</DialogTitle>
            <DialogDescription className='text-[11px]'>
              Select a commit to restore to it. A new commit will be created with those file
              contents.
            </DialogDescription>
          </DialogHeader>
          <div className='max-h-[60vh] overflow-y-auto'>
            {isLoading ? (
              <div className='flex items-center justify-center py-10 text-muted-foreground text-xs'>
                <Spinner className='size-4' />
              </div>
            ) : commits.length === 0 ? (
              <div className='flex items-center justify-center py-10 text-muted-foreground text-xs'>
                No commits found
              </div>
            ) : (
              <>
                {commits.map((c) => (
                  <CommitRow
                    key={c.hash}
                    commit={c}
                    resettingHash={reset.resettingHash}
                    onReset={reset.resetToCommit}
                    alwaysShowAction
                  />
                ))}
                <div ref={sentinelRef} className='h-6' />
                {isFetchingNextPage && (
                  <div className='flex items-center justify-center py-3 text-muted-foreground text-xs'>
                    <Spinner className='size-3' />
                  </div>
                )}
                {!hasNextPage && (
                  <div className='py-3 text-center text-[11px] text-muted-foreground'>
                    End of history
                  </div>
                )}
              </>
            )}
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}

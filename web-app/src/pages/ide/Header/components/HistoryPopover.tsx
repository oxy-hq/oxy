import { History } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/shadcn/popover";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useAuth } from "@/contexts/AuthContext";
import { useRecentCommits } from "../hooks/useRecentCommits";
import { CommitRow } from "./CommitRow";
import { HistoryDialog } from "./HistoryDialog";
import { RestoreConfirmDialog } from "./RestoreConfirmDialog";

interface Props {
  workspaceId?: string;
  branch?: string;
  onResetSuccess?: () => Promise<void> | void;
}

export function HistoryPopover({ workspaceId, branch, onResetSuccess }: Props) {
  const { isLocalMode } = useAuth();
  const [showAll, setShowAll] = useState(false);
  const {
    open,
    onOpenChange,
    setOpen,
    commits,
    hasMore,
    loading,
    resettingHash,
    pendingReset,
    resetToCommit,
    confirmReset,
    cancelReset
  } = useRecentCommits({
    workspaceId,
    branch,
    onResetSuccess
  });

  if (isLocalMode) return null;

  const handleShowAll = () => {
    setOpen(false);
    setShowAll(true);
  };

  return (
    <>
      {pendingReset && (
        <RestoreConfirmDialog
          open
          shortHash={pendingReset.shortHash}
          dirty={pendingReset.dirty}
          discardedCommits={pendingReset.discardedCommits}
          loading={resettingHash === pendingReset.hash}
          onConfirm={confirmReset}
          onCancel={cancelReset}
        />
      )}
      <HistoryDialog
        open={showAll}
        onOpenChange={setShowAll}
        workspaceId={workspaceId}
        branch={branch}
        onResetSuccess={onResetSuccess}
      />
      <Popover open={open} onOpenChange={onOpenChange}>
        <PopoverTrigger asChild>
          <Button size='sm' variant='outline' title='View commit history'>
            <History />
            History
          </Button>
        </PopoverTrigger>
        <PopoverContent className='w-80 p-0' align='end' sideOffset={6}>
          <div className='border-b px-3 py-2'>
            <p className='font-medium text-sm'>Local branch history</p>
            <p className='text-[11px] text-muted-foreground'>
              Newest first, as committed on this branch. Select a commit to restore to it — a new
              commit will be created with those file contents.
            </p>
          </div>
          <div className='max-h-72 overflow-y-auto'>
            {loading ? (
              <div className='flex items-center justify-center py-6 text-muted-foreground text-xs'>
                <Spinner className='size-3' />
              </div>
            ) : commits.length === 0 ? (
              <div className='flex items-center justify-center py-6 text-muted-foreground text-xs'>
                No commits found
              </div>
            ) : (
              commits.map((c) => (
                <CommitRow
                  key={c.hash}
                  commit={c}
                  resettingHash={resettingHash}
                  onReset={resetToCommit}
                />
              ))
            )}
          </div>
          {hasMore && !loading && commits.length > 0 && (
            <div className='border-t'>
              <button
                type='button'
                onClick={handleShowAll}
                className='w-full px-3 py-2 text-center text-[11px] text-muted-foreground transition-colors hover:bg-accent/40 hover:text-foreground'
              >
                Show all
              </button>
            </div>
          )}
        </PopoverContent>
      </Popover>
    </>
  );
}

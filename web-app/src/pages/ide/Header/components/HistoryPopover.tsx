import { History, RotateCcw } from "lucide-react";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/shadcn/popover";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useRecentCommits } from "../hooks/useRecentCommits";

interface Props {
  workspaceId?: string;
  branch?: string;
  /** Called after a successful reset so the diff/revision panels can re-fetch. */
  onResetSuccess?: () => Promise<void> | void;
}

/**
 * Popover showing recent commits on the current branch with a "Restore"
 * action that resets the branch to a chosen commit (creates a new commit
 * with that file content rather than rewriting history).
 */
export function HistoryPopover({ workspaceId, branch, onResetSuccess }: Props) {
  const { open, onOpenChange, commits, loading, resettingHash, resetToCommit } = useRecentCommits({
    workspaceId,
    branch,
    onResetSuccess
  });

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>
        <button
          type='button'
          title='View commit history'
          className='flex h-7 items-center gap-1 rounded border border-border/50 px-2 text-muted-foreground text-xs transition-colors hover:border-border hover:bg-accent/40 hover:text-foreground'
        >
          <History className='h-3 w-3' />
          History
        </button>
      </PopoverTrigger>
      <PopoverContent className='w-80 p-0' align='end' sideOffset={6}>
        <div className='border-b px-3 py-2'>
          <p className='font-medium text-sm'>Recent commits</p>
          <p className='text-[11px] text-muted-foreground'>
            Select a commit to restore to it. A new commit will be created with those file contents.
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
              <div
                key={c.hash}
                className='group flex items-start gap-2 border-b px-3 py-2 last:border-0 hover:bg-accent/40'
              >
                <div className='min-w-0 flex-1'>
                  <p className='truncate text-xs'>{c.message}</p>
                  <p className='font-mono text-[10px] text-muted-foreground'>
                    {c.short_hash} · {c.author} · {c.date}
                  </p>
                </div>
                <button
                  type='button'
                  onClick={() => resetToCommit(c.hash)}
                  disabled={!!resettingHash}
                  title={`Restore to ${c.short_hash}`}
                  className='mt-0.5 hidden shrink-0 items-center gap-1 rounded bg-primary px-1.5 py-0.5 text-[10px] text-primary-foreground transition-colors hover:bg-primary/80 disabled:opacity-50 group-hover:flex'
                >
                  <RotateCcw className='h-2.5 w-2.5' />
                  {resettingHash === c.hash ? "…" : "Restore"}
                </button>
              </div>
            ))
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}

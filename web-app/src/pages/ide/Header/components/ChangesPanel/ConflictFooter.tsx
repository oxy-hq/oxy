import { Play, X } from "lucide-react";
import { CanWorkspaceEditor } from "@/components/auth/Can";
import { Spinner } from "@/components/ui/shadcn/spinner";

interface Props {
  unresolvedCount: number;
  allResolved: boolean;
  isAborting: boolean;
  isContinuing: boolean;
  onContinue: () => void;
  onAbort: () => void;
  canContinue: boolean;
  canAbort: boolean;
}

export function ConflictFooter({
  unresolvedCount,
  allResolved,
  isAborting,
  isContinuing,
  onContinue,
  onAbort,
  canContinue,
  canAbort
}: Props) {
  return (
    <CanWorkspaceEditor>
      <div className='flex items-center gap-2 border-border/40 border-t px-3 py-2'>
        <button
          type='button'
          onClick={onContinue}
          disabled={!allResolved || isContinuing || !canContinue}
          data-testid='changes-panel-continue-button'
          title={
            allResolved
              ? undefined
              : `Resolve ${unresolvedCount} remaining file${unresolvedCount === 1 ? "" : "s"} first`
          }
          className='flex items-center gap-1.5 rounded bg-gradient-to-b from-[var(--blue-500)] to-[var(--blue-600)] px-3 py-1 font-medium text-white text-xs shadow-[var(--blue-900)]/40 shadow-sm transition-all hover:from-[var(--blue-400)] hover:to-[var(--blue-500)] disabled:opacity-40'
        >
          {isContinuing ? <Spinner className='size-3' /> : <Play className='h-3 w-3' />}
          {isContinuing ? "Saving…" : "Save resolution"}
        </button>
        <button
          type='button'
          onClick={onAbort}
          disabled={isAborting || !canAbort}
          className='flex items-center gap-1.5 rounded border border-destructive/30 px-3 py-1 text-destructive text-xs transition-colors hover:bg-destructive/10 disabled:opacity-50'
        >
          {isAborting ? <Spinner className='size-3' /> : <X className='h-3 w-3' />}
          {isAborting ? "Aborting…" : "Abort"}
        </button>
        {unresolvedCount > 0 && (
          <span className='ml-auto font-mono text-[10px] text-muted-foreground/50'>
            {unresolvedCount} file{unresolvedCount === 1 ? "" : "s"} remaining
          </span>
        )}
      </div>
    </CanWorkspaceEditor>
  );
}

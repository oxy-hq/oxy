import { ChevronDown, Download, GitMerge, GitPullRequest, RefreshCw, Upload } from "lucide-react";
import { CanWorkspaceAdmin, CanWorkspaceEditor } from "@/components/auth/Can";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";

import type { CtaState } from "./ctaState";

interface Props {
  state: CtaState;
  pushLabel: string;
  prUrl: string | null;
  /** Show the split ▼ menu next to the primary button (only on remote branches with a primary action). */
  showSplit: boolean;
  isPushing: boolean;
  isForcePushing: boolean;
  isFetching: boolean;
  onOpenChanges: () => void;
  onOpenPullDialog: () => void;
  onPushDirect: () => void;
  onForcePush: () => void;
  onFetch: () => void;
}

const SPLIT_TRIGGER_CLASS =
  "border-l border-white/20 bg-gradient-to-b from-[var(--blue-500)] to-[var(--blue-600)] text-white hover:from-[var(--blue-400)] hover:to-[var(--blue-500)]";

const PRIMARY_BUTTON_CLASS =
  "flex items-center gap-1 bg-gradient-to-b from-[var(--blue-500)] to-[var(--blue-600)] px-2.5 font-medium text-white text-xs shadow-[var(--blue-900)]/40 shadow-sm transition-all hover:from-[var(--blue-400)] hover:to-[var(--blue-500)] disabled:opacity-50";

/**
 * State-driven primary call-to-action cluster shown after the branch pill.
 * The `state` enum drives which button (or pair of buttons) renders. The
 * decision logic lives in the parent (`GitActions`); this component is
 * purely presentational + click handlers.
 */
export function PrimaryCta({
  state,
  pushLabel,
  prUrl,
  showSplit,
  isPushing,
  isForcePushing,
  isFetching,
  onOpenChanges,
  onOpenPullDialog,
  onPushDirect,
  onForcePush,
  onFetch
}: Props) {
  if (state === "none") return null;

  const radius = showSplit ? "rounded-l" : "rounded";

  return (
    <div className='flex h-7 items-stretch'>
      {state === "conflict" && (
        <button
          type='button'
          onClick={onOpenChanges}
          className='flex items-center gap-1 rounded border border-warning/30 bg-warning/10 px-2.5 text-warning text-xs transition-colors hover:border-warning/50 hover:bg-warning/20'
        >
          <GitMerge className='h-3 w-3' />
          Conflict
        </button>
      )}

      {state === "commit" && (
        <CanWorkspaceEditor>
          <button
            type='button'
            onClick={onOpenChanges}
            disabled={isPushing}
            data-testid='ide-commit-push-button'
            className={`${PRIMARY_BUTTON_CLASS} ${radius}`}
          >
            <Upload className='h-3 w-3' />
            {pushLabel}
          </button>
        </CanWorkspaceEditor>
      )}

      {state === "push" && (
        <CanWorkspaceEditor>
          <button
            type='button'
            onClick={onPushDirect}
            disabled={isPushing}
            data-testid='ide-push-button'
            className={`${PRIMARY_BUTTON_CLASS} ${radius}`}
          >
            <Upload className='h-3 w-3' />
            {isPushing ? "Pushing…" : "Push"}
          </button>
        </CanWorkspaceEditor>
      )}

      {state === "pull" && (
        <CanWorkspaceEditor>
          <button
            type='button'
            onClick={onOpenPullDialog}
            data-testid='ide-pull-button'
            className='flex items-center gap-1 rounded-l border border-border/50 px-2.5 text-muted-foreground text-xs transition-colors hover:border-border hover:bg-accent/40 hover:text-foreground'
          >
            <Download className='h-3 w-3' />
            Pull
          </button>
          <CanWorkspaceAdmin>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button
                  type='button'
                  aria-label='More actions'
                  className='flex w-5 items-center justify-center rounded-r border border-border/50 border-l-0 text-muted-foreground text-xs transition-colors hover:border-border hover:bg-accent/40 hover:text-foreground'
                >
                  <ChevronDown className='h-2.5 w-2.5' />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align='end' className='w-auto min-w-0'>
                <DropdownMenuItem
                  onClick={onForcePush}
                  disabled={isForcePushing}
                  className='gap-1.5 px-2 py-1 text-destructive text-xs focus:text-destructive'
                >
                  <Upload className='h-3 w-3' />
                  {isForcePushing ? "Pushing…" : "Force Push"}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </CanWorkspaceAdmin>
        </CanWorkspaceEditor>
      )}

      {state === "pr" && prUrl && (
        <a
          href={prUrl}
          target='_blank'
          rel='noopener noreferrer'
          title='Open pull request on GitHub'
          className={`${PRIMARY_BUTTON_CLASS} ${radius}`}
        >
          <GitPullRequest className='h-3 w-3' />
          Open PR
        </a>
      )}

      {state === "fetch" && (
        <button
          type='button'
          onClick={onFetch}
          disabled={isFetching}
          title='Fetch from origin'
          data-testid='ide-git-refresh-button'
          className='flex h-7 items-center gap-1 rounded border border-border/50 px-2.5 text-muted-foreground text-xs transition-colors hover:border-border hover:bg-accent/40 hover:text-foreground disabled:opacity-40'
        >
          <RefreshCw className={`h-3 w-3 ${isFetching ? "animate-spin" : ""}`} />
          Fetch
        </button>
      )}

      {showSplit && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type='button'
              aria-label='More actions'
              className={`flex w-5 items-center justify-center rounded-r text-xs transition-all ${SPLIT_TRIGGER_CLASS}`}
            >
              <ChevronDown className='h-2.5 w-2.5' />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align='end' className='w-auto min-w-0'>
            <DropdownMenuItem
              onClick={onFetch}
              disabled={isFetching}
              className='gap-1.5 px-2 py-1 text-xs'
            >
              <RefreshCw className={`h-3 w-3 ${isFetching ? "animate-spin" : ""}`} />
              {isFetching ? "Fetching…" : "Fetch"}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}

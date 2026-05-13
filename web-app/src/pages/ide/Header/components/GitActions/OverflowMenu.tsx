import {
  ExternalLink,
  GitPullRequest,
  MoreHorizontal,
  RefreshCw,
  Trash2,
  Upload,
  X
} from "lucide-react";
import { CanWorkspaceAdmin } from "@/components/auth/Can";
import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";
import type { GitState } from "./useGitState";

interface Props {
  state: GitState;
  prUrl: string | undefined;
  isFetching: boolean;
  isForcePushing: boolean;
  onFetch: () => void;
  onForcePush: () => void;
  onAbortRebase: () => void;
  onDiscardAll: () => void;
}

export function OverflowMenu({
  state,
  prUrl,
  isFetching,
  isForcePushing,
  onFetch,
  onForcePush,
  onAbortRebase,
  onDiscardAll
}: Props) {
  const { hasRemote, isInConflict, aheadCount, uncommittedCount, caps } = state;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button size='sm' variant='ghost' aria-label='More git actions' className='h-8 w-8 p-0'>
          <MoreHorizontal />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align='end' className='min-w-[14rem]'>
        <DropdownMenuItem onClick={onFetch} disabled={!hasRemote || isFetching}>
          <RefreshCw className={isFetching ? "animate-spin" : ""} />
          Refresh from remote
        </DropdownMenuItem>
        {prUrl && (
          <DropdownMenuItem asChild>
            <a href={prUrl} target='_blank' rel='noopener noreferrer'>
              <GitPullRequest />
              Open PR
              <ExternalLink className='ml-auto h-3.5 w-3.5 opacity-60' />
            </a>
          </DropdownMenuItem>
        )}

        <CanWorkspaceAdmin>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            onClick={onForcePush}
            disabled={!caps.can_force_push || (aheadCount === 0 && !isInConflict) || isForcePushing}
            className='text-status-warning-text focus:text-status-warning-text'
          >
            <Upload />
            {isForcePushing ? "Pushing…" : "Force push"}
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={onAbortRebase}
            disabled={!isInConflict}
            className='text-status-warning-text focus:text-status-warning-text'
          >
            <X />
            Abort rebase
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={onDiscardAll}
            disabled={uncommittedCount === 0 && !isInConflict}
            className='text-destructive focus:text-destructive'
          >
            <Trash2 />
            Discard all changes
          </DropdownMenuItem>
        </CanWorkspaceAdmin>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

import { Download } from "lucide-react";
import { CanWorkspaceEditor } from "@/components/auth/Can";
import { Button } from "@/components/ui/shadcn/button";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger
} from "@/components/ui/shadcn/tooltip";
import type { GitState } from "./useGitState";

interface Props {
  state: GitState;
  onClick: () => void;
}

// Trigger only — the pull mutation lives in PullDialog. Do not flow the
// 15s revision-info poll into `disabled` or the button visibly disables
// every tick.
//
// Disabled `<button>` swallows pointer events, blocking the Radix tooltip
// trigger — wrap in a `<span>` that catches hover/focus.
export function PullButton({ state, onClick }: Props) {
  if (!state.caps.can_pull || !state.hasRemote) return null;
  if (state.behindCount === 0) return null;

  const { enabled, tooltip } = pullPredicate(state);
  const disabled = !enabled;

  const button = (
    <Button
      size='sm'
      variant='outline'
      onClick={onClick}
      disabled={disabled}
      data-testid='ide-pull-button'
    >
      <Download />
      Pull
      {state.behindCount > 0 && (
        <span className='ml-0.5 rounded bg-primary/10 px-1.5 py-0.5 font-medium text-primary text-xs'>
          {state.behindCount}
        </span>
      )}
    </Button>
  );

  return (
    <CanWorkspaceEditor>
      {tooltip ? (
        <TooltipProvider delayDuration={300}>
          <Tooltip>
            <TooltipTrigger asChild>
              <span className='inline-flex'>{button}</span>
            </TooltipTrigger>
            <TooltipContent>{tooltip}</TooltipContent>
          </Tooltip>
        </TooltipProvider>
      ) : (
        button
      )}
    </CanWorkspaceEditor>
  );
}

function pullPredicate(state: GitState): { enabled: boolean; tooltip: string | undefined } {
  if (state.isInConflict) return { enabled: false, tooltip: "Resolve the conflict first" };
  if (state.uncommittedCount > 0)
    return { enabled: false, tooltip: "Commit or discard local changes first" };
  return { enabled: true, tooltip: undefined };
}

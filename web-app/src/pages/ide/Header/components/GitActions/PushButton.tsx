import { Upload } from "lucide-react";
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
  isPushing: boolean;
  onClick: () => void;
}

// Disabled `<button>` swallows pointer events, blocking the Radix tooltip
// trigger — wrap in a `<span>` that catches hover/focus on the button's
// behalf.
export function PushButton({ state, isPushing, onClick }: Props) {
  if (!state.caps.can_push || !state.hasRemote) return null;
  if (state.aheadCount === 0) return null;

  const { enabled, tooltip } = pushPredicate(state);
  const disabled = !enabled || isPushing;

  const button = (
    <Button size='sm' onClick={onClick} disabled={disabled} data-testid='ide-push-button'>
      <Upload />
      {isPushing ? "Pushing…" : "Push"}
      {state.aheadCount > 0 && (
        <span className='ml-0.5 rounded bg-primary-foreground/20 px-1.5 py-0.5 font-medium text-xs'>
          {state.aheadCount}
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

function pushPredicate(state: GitState): { enabled: boolean; tooltip: string | undefined } {
  if (state.isInConflict) return { enabled: false, tooltip: "Resolve the conflict first" };
  if (state.isProtected)
    return { enabled: false, tooltip: "Branch is protected — create a feature branch" };
  return { enabled: true, tooltip: undefined };
}

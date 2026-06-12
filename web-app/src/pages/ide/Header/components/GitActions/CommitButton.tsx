import { GitCommitHorizontal } from "lucide-react";
import { CanWorkspaceEditor } from "@/components/auth/Can";
import { Button } from "@/components/ui/shadcn/button";
import type { GitState } from "./useGitState";

interface Props {
  state: GitState;
  isPushing: boolean;
  onClick: () => void;
}

export function CommitButton({ state, isPushing, onClick }: Props) {
  if (!state.caps.can_commit || state.uncommittedCount === 0 || state.isInConflict) return null;

  const label = state.caps.can_push ? "Commit & Push" : "Commit";

  return (
    <CanWorkspaceEditor>
      <Button
        size='sm'
        onClick={onClick}
        disabled={isPushing}
        data-testid='ide-commit-button'
        className='h-7 gap-1 px-2 text-xs [&>svg]:size-3.5'
      >
        <GitCommitHorizontal />
        {label}
        <span className='ml-0.5 rounded bg-primary-foreground/20 px-1 py-px font-medium text-[10px]'>
          {state.uncommittedCount}
        </span>
      </Button>
    </CanWorkspaceEditor>
  );
}

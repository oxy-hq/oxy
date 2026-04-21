import { ChevronDown, GitBranch } from "lucide-react";
import { useState } from "react";
import { useAuth } from "@/contexts/AuthContext";
import type { RevisionInfo } from "@/types/settings";
import { BranchInfo } from "../../BranchInfo";
import { BranchQuickSwitcher } from "../../BranchQuickSwitcher";
import { HistoryPopover } from "../HistoryPopover";
import { deriveCtaState } from "./ctaState";
import { PrimaryCta } from "./PrimaryCta";

interface Props {
  workspaceId?: string;
  branch: string;
  isOnMain: boolean;

  // Capability flags (already AND-ed against caps elsewhere)
  canCommit: boolean;
  canBrowseHistory: boolean;
  canPush: boolean;
  canPull: boolean;

  // Diff / revision derived state
  hasLocalChanges: boolean;
  revisionInfo?: RevisionInfo;
  prUrl: string | null;

  pushLabel: string;
  isPushing: boolean;
  isForcePushing: boolean;
  isFetching: boolean;

  onOpenChanges: () => void;
  onOpenPullDialog: () => void;
  onPushDirect: () => void;
  onForcePush: () => void;
  onFetch: () => void;
  onResetSuccess: () => Promise<void> | void;
}

/**
 * The primary git actions cluster shown in the IDE header for the workspace
 * (non-linked-repo) flow. Renders a "Local mode" label when there is no git
 * repo; otherwise composes the branch pill, optional history popover, an
 * on-main "new branch" shortcut, and the state-driven primary CTA.
 *
 * State derivations (`ctaState`, `showSplit`, `showOpenPr`) are computed
 * here so the parent only has to pass capability flags + raw revision data.
 */
export function GitActions({
  workspaceId,
  branch,
  isOnMain,
  canCommit,
  canBrowseHistory,
  canPush,
  canPull,
  hasLocalChanges,
  revisionInfo,
  prUrl,
  pushLabel,
  isPushing,
  isForcePushing,
  isFetching,
  onOpenChanges,
  onOpenPullDialog,
  onPushDirect,
  onForcePush,
  onFetch,
  onResetSuccess
}: Props) {
  const { isLocalMode } = useAuth();
  const [isBranchPickerOpen, setIsBranchPickerOpen] = useState(false);

  if (isLocalMode) return null;

  if (!canCommit) {
    return <div className='text-muted-foreground text-sm'>Local mode</div>;
  }

  const isBehind = revisionInfo?.sync_status === "behind";
  const isAhead = revisionInfo?.sync_status === "ahead";
  const isConflict = revisionInfo?.sync_status === "conflict";
  const showPull = canPull && isBehind && !isConflict;
  const showOpenPr = !isOnMain && !!prUrl && !hasLocalChanges && !isConflict && !isAhead;

  const ctaState = deriveCtaState({
    isOnMain,
    isAhead,
    isConflict,
    hasLocalChanges,
    showPull,
    showOpenPr,
    canPush
  });

  const showSplit =
    canPush && (ctaState === "commit" || ctaState === "push" || ctaState === "pr") && !isOnMain;

  const branchPill = (
    <button
      type='button'
      className='flex h-7 max-w-36 items-center gap-1.5 overflow-hidden rounded border border-border/50 bg-transparent px-2 text-sm transition-colors hover:border-border hover:bg-accent/40'
    >
      <span className='min-w-0 flex-1'>
        <BranchInfo />
      </span>
      <ChevronDown className='h-3 w-3 flex-shrink-0 text-muted-foreground/60' />
    </button>
  );

  return (
    <div className='flex items-center gap-1.5'>
      <BranchQuickSwitcher
        trigger={branchPill}
        open={isBranchPickerOpen}
        onOpenChange={setIsBranchPickerOpen}
      />

      {canBrowseHistory && (
        <HistoryPopover workspaceId={workspaceId} branch={branch} onResetSuccess={onResetSuccess} />
      )}

      <div className='mx-0.5 h-4 w-px bg-border/50' />

      {isOnMain && !hasLocalChanges && (
        <button
          type='button'
          onClick={() => setIsBranchPickerOpen(true)}
          title='Create a new branch'
          className='flex h-7 items-center gap-1 rounded border border-primary/30 bg-primary/8 px-2 text-primary text-xs transition-colors hover:border-primary/50 hover:bg-primary/15'
        >
          <GitBranch className='h-3 w-3' />
          New branch
        </button>
      )}

      <PrimaryCta
        state={ctaState}
        pushLabel={pushLabel}
        prUrl={prUrl}
        showSplit={showSplit}
        isPushing={isPushing}
        isForcePushing={isForcePushing}
        isFetching={isFetching}
        onOpenChanges={onOpenChanges}
        onOpenPullDialog={onOpenPullDialog}
        onPushDirect={onPushDirect}
        onForcePush={onForcePush}
        onFetch={onFetch}
      />
    </div>
  );
}

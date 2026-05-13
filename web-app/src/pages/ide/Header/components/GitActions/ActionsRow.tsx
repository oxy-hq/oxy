import { useState } from "react";
import { useIdeGit } from "../../context/IdeGitContext";
import { ChangesPanel } from "../ChangesPanel";
import { PullDialog } from "../PullDialog";
import { CommitButton } from "./CommitButton";
import { DiscardAllConfirmDialog } from "./DiscardAllConfirmDialog";
import { ForcePushConfirmDialog } from "./ForcePushConfirmDialog";
import { OverflowMenu } from "./OverflowMenu";
import { PullButton } from "./PullButton";
import { PushButton } from "./PushButton";
import { ResolveButton } from "./ResolveButton";

export function ActionsRow() {
  const { gitState, branch, status, actions, prUrl, refresh } = useIdeGit();
  const [pullDialogOpen, setPullDialogOpen] = useState(false);
  const [changesPanelOpen, setChangesPanelOpen] = useState(false);
  const [discardOpen, setDiscardOpen] = useState(false);
  const [forcePushOpen, setForcePushOpen] = useState(false);

  const { isFetching, isPushing, isForcePushing, isDiscarding } = status;
  const { push, forcePush, abortRebase, continueRebase, fetchRemote, discardAll } = actions;

  const openChanges = () => setChangesPanelOpen(true);
  const pushLabel = gitState.caps.can_commit
    ? gitState.caps.can_push
      ? "Commit & Push"
      : "Commit"
    : "Push";

  return (
    <div className='flex items-center gap-1'>
      <PullDialog
        open={pullDialogOpen}
        onOpenChange={setPullDialogOpen}
        onConflict={() => setChangesPanelOpen(true)}
      />
      <ChangesPanel
        open={changesPanelOpen}
        onOpenChange={setChangesPanelOpen}
        isPushing={isPushing}
        pushLabel={pushLabel}
        onPush={push}
        isConflict={gitState.isInConflict}
        onAbortConflict={async () => {
          await abortRebase();
          setChangesPanelOpen(false);
        }}
        onContinueRebase={async () => {
          await continueRebase();
          setChangesPanelOpen(false);
        }}
        onConflictResolved={() => {
          void refresh();
        }}
      />
      <DiscardAllConfirmDialog
        open={discardOpen}
        onOpenChange={setDiscardOpen}
        uncommittedCount={gitState.uncommittedCount}
        isInConflict={gitState.isInConflict}
        isPending={isDiscarding}
        onConfirm={async () => {
          await discardAll();
          setDiscardOpen(false);
        }}
      />
      <ForcePushConfirmDialog
        open={forcePushOpen}
        onOpenChange={setForcePushOpen}
        branch={branch}
        aheadCount={gitState.aheadCount}
        isInConflict={gitState.isInConflict}
        isPending={isForcePushing}
        onConfirm={async () => {
          await forcePush();
          setForcePushOpen(false);
        }}
      />

      {gitState.isInConflict ? (
        <ResolveButton onClick={openChanges} />
      ) : (
        <>
          <PullButton state={gitState} onClick={() => setPullDialogOpen(true)} />
          <PushButton state={gitState} isPushing={isPushing} onClick={() => void push("")} />
        </>
      )}

      <CommitButton state={gitState} isPushing={isPushing} onClick={openChanges} />

      <OverflowMenu
        state={gitState}
        prUrl={prUrl ?? undefined}
        isFetching={isFetching}
        isForcePushing={isForcePushing}
        onFetch={() => void fetchRemote()}
        onForcePush={() => setForcePushOpen(true)}
        onAbortRebase={() => void abortRebase()}
        onDiscardAll={() => setDiscardOpen(true)}
      />
    </div>
  );
}

import { useState } from "react";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/shadcn/dialog";
import type { WorkspaceCreationType } from "../../types";
import WorkspaceCreator, { type WorkspaceCreationPhase } from "../WorkspaceCreator";

interface Props {
  open: boolean;
  onClose: () => void;
  /** Optional side-effect hook (e.g. refetch list). The dialog hosts the
   *  full create → preparing flow and auto-navigation on ready. */
  onCreated?: (workspaceId: string, type: WorkspaceCreationType) => void;
}

export function CreateWorkspaceDialog({ open, onClose, onCreated }: Props) {
  const [phase, setPhase] = useState<WorkspaceCreationPhase>("create");
  // Closing the dialog unmounts WorkspaceCreator (Radix removes portal
  // content when `open` is false), which in turn unmounts WorkspacePreparing
  // and cancels its auto-redirect timers.

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle className='font-semibold text-base'>
            {phase === "preparing" ? "Preparing your workspace" : "New workspace"}
          </DialogTitle>
        </DialogHeader>

        <WorkspaceCreator onCreated={onCreated} onPhaseChange={setPhase} />
      </DialogContent>
    </Dialog>
  );
}

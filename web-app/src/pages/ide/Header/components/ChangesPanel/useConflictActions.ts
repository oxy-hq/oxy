import { useState } from "react";
import { toast } from "sonner";
import { WorkspaceService as ProjectService } from "@/services/api/workspaces";

interface Args {
  workspaceId: string | undefined;
  branchName: string;
  onResolved?: () => void;
  onAbortConflict?: () => Promise<void>;
  onContinueRebase?: () => Promise<void>;
}

export function useConflictActions({
  workspaceId,
  branchName,
  onResolved,
  onAbortConflict,
  onContinueRebase
}: Args) {
  const [resolvingFile, setResolvingFile] = useState<{
    path: string;
    side: "mine" | "theirs";
  } | null>(null);
  const [unresolvingPath, setUnresolvingPath] = useState<string | null>(null);
  const [isAborting, setIsAborting] = useState(false);
  const [isContinuing, setIsContinuing] = useState(false);

  const handleResolveFile = async (filePath: string, side: "mine" | "theirs") => {
    if (!workspaceId || !branchName) return;
    setResolvingFile({ path: filePath, side });
    try {
      const result = await ProjectService.resolveConflictFile(
        workspaceId,
        branchName,
        filePath,
        side
      );
      if (result.success) {
        onResolved?.();
      } else {
        toast.error("Failed to resolve file", {
          action: result.message
            ? {
                label: "Show details",
                onClick: () => toast.message(result.message)
              }
            : undefined
        });
      }
    } catch {
      toast.error("Failed to resolve file");
    } finally {
      setResolvingFile(null);
    }
  };

  const handleUnresolveFile = async (filePath: string) => {
    if (!workspaceId || !branchName) return;
    setUnresolvingPath(filePath);
    try {
      const result = await ProjectService.unresolveConflictFile(workspaceId, branchName, filePath);
      if (result.success) {
        onResolved?.();
      } else {
        toast.error("Failed to undo resolution", {
          action: result.message
            ? {
                label: "Show details",
                onClick: () => toast.message(result.message)
              }
            : undefined
        });
      }
    } catch {
      toast.error("Failed to undo resolution");
    } finally {
      setUnresolvingPath(null);
    }
  };

  const handleContinue = async () => {
    if (!onContinueRebase) return;
    setIsContinuing(true);
    try {
      await onContinueRebase();
    } finally {
      setIsContinuing(false);
    }
  };

  const handleAbort = async () => {
    if (!onAbortConflict) return;
    setIsAborting(true);
    try {
      await onAbortConflict();
    } finally {
      setIsAborting(false);
    }
  };

  return {
    resolvingFile,
    unresolvingPath,
    isAborting,
    isContinuing,
    handleResolveFile,
    handleUnresolveFile,
    handleContinue,
    handleAbort
  };
}

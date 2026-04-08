import { useQueryClient } from "@tanstack/react-query";
import { CheckCircle, RotateCcw } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import queryKeys from "@/hooks/api/queryKey";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { WorkspaceService as ProjectService } from "@/services/api/workspaces";

const ConflictPanel = ({ branch }: { remoteUrl?: string; branch: string }) => {
  const { project } = useCurrentProjectBranch();
  const queryClient = useQueryClient();
  const [isAborting, setIsAborting] = useState(false);
  const [isContinuing, setIsContinuing] = useState(false);

  const invalidate = () =>
    queryClient.invalidateQueries({
      queryKey: queryKeys.workspaces.revisionInfo(project.id, branch)
    });

  const handleContinue = async () => {
    setIsContinuing(true);
    try {
      const result = await ProjectService.continueRebase(project.id, branch);
      if (result.success) {
        toast.success("Rebase continued — conflict resolved");
        invalidate();
      } else {
        toast.error(result.message || "Failed to continue rebase");
      }
    } catch {
      toast.error("Failed to continue rebase");
    } finally {
      setIsContinuing(false);
    }
  };

  const handleAbort = async () => {
    setIsAborting(true);
    try {
      const result = await ProjectService.abortRebase(project.id, branch);
      if (result.success) {
        toast.success("Rebase aborted — branch restored to previous state");
        invalidate();
      } else {
        toast.error(result.message || "Failed to abort rebase");
      }
    } catch {
      toast.error("Failed to abort rebase");
    } finally {
      setIsAborting(false);
    }
  };

  return (
    <div className='rounded-md border border-warning/30 bg-warning/5 p-3 text-sm'>
      <p className='font-medium text-warning'>Merge conflict detected</p>
      <p className='mt-1 text-muted-foreground text-xs'>
        Open the conflicted files below, resolve the conflict markers (
        <code className='font-mono'>{"<<<<<<< / ======= / >>>>>>>"}</code>), then click{" "}
        <strong>Fix conflicts</strong>.
      </p>
      <div className='mt-3 flex flex-wrap gap-2'>
        <Button
          variant='outline'
          size='sm'
          className='h-7 gap-1.5 text-xs'
          onClick={handleContinue}
          disabled={isContinuing || isAborting}
        >
          <CheckCircle className='h-3 w-3' />
          {isContinuing ? "Fixing…" : "Fix conflicts"}
        </Button>
        <Button
          variant='outline'
          size='sm'
          className='h-7 gap-1.5 text-destructive text-xs hover:text-destructive'
          onClick={handleAbort}
          disabled={isAborting || isContinuing}
        >
          <RotateCcw className='h-3 w-3' />
          {isAborting ? "Aborting…" : "Abort"}
        </Button>
      </div>
    </div>
  );
};

export default ConflictPanel;

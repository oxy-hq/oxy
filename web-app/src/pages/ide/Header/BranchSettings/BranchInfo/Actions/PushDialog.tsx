import { Loader2 } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { usePushChanges } from "@/hooks/api/projects/useProjects";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";

interface PushDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export const PushDialog = ({ open, onOpenChange }: PushDialogProps) => {
  const { project, branchName } = useCurrentProjectBranch();
  const pushChangesMutation = usePushChanges();
  const [commitMessage, setCommitMessage] = useState("Auto-commit: Oxy changes");
  const navigate = useNavigate();

  const onConfirm = async (e: { preventDefault: () => void }) => {
    e.preventDefault();
    if (!project?.id || !branchName) {
      toast.error("Project or branch information is missing");
      return;
    }

    try {
      const result = await pushChangesMutation.mutateAsync({
        projectId: project.id,
        branchName,
        commitMessage: commitMessage.trim() || "Auto-commit: Oxy changes"
      });

      if (result.success) {
        toast.success(result.message || "Changes pushed successfully");
        const ideUri = ROUTES.PROJECT(project.id).IDE.ROOT;
        navigate(ideUri);
      } else {
        toast.error(result.message || "Failed to push changes");
      }
    } catch (error) {
      toast.error("Failed to push changes");
      console.error("Push changes error:", error);
    } finally {
      onOpenChange(false);
      setCommitMessage("Auto-commit: Oxy changes");
    }
  };

  const handleCancel = () => {
    onOpenChange(false);
    setCommitMessage("Auto-commit: Oxy changes");
  };

  const isDisabled = pushChangesMutation.isPending || !project?.id || !branchName;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Push Changes</DialogTitle>
        </DialogHeader>
        <div className='space-y-4 py-4'>
          <p className='text-muted-foreground text-sm'>
            This will push all local changes to the remote repository and force update the remote
            branch.
          </p>
          <div className='space-y-2'>
            <Label htmlFor='commit-message'>Commit Message (Optional)</Label>
            <Input
              id='commit-message'
              value={commitMessage}
              onChange={(e) => setCommitMessage(e.target.value)}
              placeholder='Enter commit message...'
              disabled={isDisabled}
            />
          </div>
        </div>
        <DialogFooter>
          <Button variant='outline' onClick={handleCancel} disabled={isDisabled}>
            Cancel
          </Button>
          <Button onClick={onConfirm} disabled={isDisabled}>
            {pushChangesMutation.isPending && <Loader2 className='mr-2 h-4 w-4 animate-spin' />}
            Push Changes
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

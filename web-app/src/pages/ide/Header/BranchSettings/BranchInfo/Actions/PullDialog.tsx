import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/shadcn/alert-dialog";
import { usePullChanges } from "@/hooks/api/projects/useProjects";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import { Loader2 } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export const PullDialog = ({ open, onOpenChange }: Props) => {
  const { project, branchName } = useCurrentProjectBranch();
  const pullChangesMutation = usePullChanges();
  const navigate = useNavigate();

  const onConfirm = async (e: { preventDefault: () => void }) => {
    e.preventDefault();

    if (!project?.id || !branchName) {
      toast.error("Project or branch information is missing");
      return;
    }

    try {
      const result = await pullChangesMutation.mutateAsync({
        projectId: project.id,
        branchName,
      });

      if (result.success) {
        toast.success(result.message || "Changes pulled successfully");
      } else {
        toast.error(result.message || "Failed to pull changes");
      }
      const ideUri = ROUTES.PROJECT(project.id).IDE.ROOT;
      navigate(ideUri);
    } catch (error) {
      toast.error("Failed to pull changes");
      console.error("Pull changes error:", error);
    } finally {
      onOpenChange(false);
    }
  };

  const isDisabled =
    pullChangesMutation.isPending || !project?.id || !branchName;

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Pull Latest Changes</AlertDialogTitle>
          <AlertDialogDescription>
            This action will discard all local changes and pull the latest from
            the remote repository. This action cannot be undone.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={isDisabled}>Cancel</AlertDialogCancel>
          <AlertDialogAction
            onClick={onConfirm}
            disabled={isDisabled}
            className="bg-destructive hover:bg-destructive/90"
          >
            {pullChangesMutation.isPending && (
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            )}
            Pull Changes
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
};

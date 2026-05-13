import { toast } from "sonner";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from "@/components/ui/shadcn/alert-dialog";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useAuth } from "@/contexts/AuthContext";
import { usePullChanges } from "@/hooks/api/workspaces/useWorkspaces";
import { useIdeGit } from "../context/IdeGitContext";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConflict?: () => void;
}

export const PullDialog = ({ open, onOpenChange, onConflict }: Props) => {
  const { isLocalMode } = useAuth();
  const { workspaceId, branch } = useIdeGit();
  const pullChangesMutation = usePullChanges();

  if (isLocalMode) return null;

  const onConfirm = async (e: { preventDefault: () => void }) => {
    e.preventDefault();

    if (!workspaceId || !branch) {
      toast.error("Workspace or branch information is missing");
      return;
    }

    try {
      const result = await pullChangesMutation.mutateAsync({
        workspaceId,
        branchName: branch
      });

      if (result.state === "Synced") {
        toast.success("Pulled latest changes");
      } else if (result.state === "Conflict") {
        toast.warning("Pull paused with conflicts", {
          description: "Resolve them in the changes panel."
        });
        onConflict?.();
      } else if (result.state === "Error") {
        toast.error("Pull failed", {
          action: result.message
            ? {
                label: "Show details",
                onClick: () => toast.message(result.message)
              }
            : undefined
        });
      }
    } catch (error) {
      toast.error("Failed to pull changes");
      console.error("Pull changes error:", error);
    } finally {
      onOpenChange(false);
    }
  };

  const isDisabled = pullChangesMutation.isPending || !workspaceId || !branch;

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Pull Latest Changes</AlertDialogTitle>
          <AlertDialogDescription>
            Fetch updates from <code>origin/{branch}</code> and rebase your local commits on top. If
            there are conflicts, you'll resolve them in the changes panel.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={isDisabled}>Cancel</AlertDialogCancel>
          <AlertDialogAction onClick={onConfirm} disabled={isDisabled}>
            {pullChangesMutation.isPending && <Spinner className='mr-2' />}
            Pull Changes
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
};

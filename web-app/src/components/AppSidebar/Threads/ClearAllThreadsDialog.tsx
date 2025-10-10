import { useCallback } from "react";
import { useNavigate } from "react-router-dom";
import useDeleteAllThread from "@/hooks/api/threads/useDeleteAllThread";
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
import ROUTES from "@/libs/utils/routes";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

interface ClearAllThreadsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const ClearAllThreadsDialog = ({
  open,
  onOpenChange,
}: ClearAllThreadsDialogProps) => {
  const navigate = useNavigate();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const { mutate: clearAllThreads } = useDeleteAllThread();

  const confirm = useCallback(() => {
    clearAllThreads(undefined, {
      onSuccess: () => {
        if (projectId) {
          navigate(ROUTES.PROJECT(projectId).THREADS);
        }
      },
    });
  }, [clearAllThreads, navigate, projectId]);

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Are you absolutely sure?</AlertDialogTitle>
          <AlertDialogDescription>
            This action cannot be undone. This will permanently delete all
            threads.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction onClick={confirm}>Continue</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
};

export default ClearAllThreadsDialog;

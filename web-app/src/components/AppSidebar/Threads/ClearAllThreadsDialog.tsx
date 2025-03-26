import { useCallback } from "react";
import { useNavigate } from "react-router-dom";
import useDeleteAllThread from "@/hooks/api/useDeleteAllThread";
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

interface ClearAllThreadsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const ClearAllThreadsDialog = ({
  open,
  onOpenChange,
}: ClearAllThreadsDialogProps) => {
  const navigate = useNavigate();
  const { mutate: clearAllThreads } = useDeleteAllThread();

  const confirm = useCallback(() => {
    clearAllThreads(undefined, {
      onSuccess: () => navigate("/threads"),
    });
  }, [clearAllThreads, navigate]);

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

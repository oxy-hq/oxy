import React from "react";
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
import { UserInfo } from "@/types/auth";

interface DeleteUserDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  user: UserInfo | null;
  currentUser: UserInfo | null;
  onConfirm: () => void;
}

export const DeleteUserDialog: React.FC<DeleteUserDialogProps> = ({
  open,
  onOpenChange,
  user,
  currentUser,
  onConfirm,
}) => {
  const isSelfDeletion = user?.id === currentUser?.id;
  const isTargetAdmin = user?.role === "admin";
  const cannotDelete =
    isSelfDeletion || (isTargetAdmin && user?.id !== currentUser?.id);

  const getDialogMessage = () => {
    if (isSelfDeletion) {
      return "You cannot delete your own account. Please ask another administrator to delete your account if needed.";
    }

    if (isTargetAdmin) {
      return "You cannot delete another administrator via the UI. Admin accounts are synced from config file.";
    }

    return (
      <>
        Are you sure you want to delete "{user?.name}"? This action cannot be
        undone, and the user will lose access to the system immediately.
      </>
    );
  };

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent className="sm:max-w-md">
        <AlertDialogHeader>
          <AlertDialogTitle>Delete User</AlertDialogTitle>
          <AlertDialogDescription>{getDialogMessage()}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>
            {cannotDelete ? "Close" : "Cancel"}
          </AlertDialogCancel>
          {!cannotDelete && (
            <AlertDialogAction
              onClick={onConfirm}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              Delete User
            </AlertDialogAction>
          )}
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
};

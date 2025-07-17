import React, { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/shadcn/dropdown-menu";
import { MoreHorizontal, Trash2, Loader2, RotateCcw } from "lucide-react";
import { UserInfo } from "@/types/auth";
import {
  useDeleteUser,
  useUpdateUser,
} from "@/hooks/api/users/useUserMutations";
import { toast } from "sonner";
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
import { buttonVariants } from "@/components/ui/shadcn/utils/button-variants";

interface Props {
  user: UserInfo;
}

const Actions: React.FC<Props> = ({ user }) => {
  const [isDeleting, setIsDeleting] = useState(false);
  const deleteUserMutation = useDeleteUser();
  const updateUserMutation = useUpdateUser();
  const [isDeleteAlertOpen, setIsDeleteAlertOpen] = useState(false);

  const handleOpenDeleteDialog = () => {
    setIsDeleteAlertOpen(true);
  };

  const onConfirmDelete = () => {
    setIsDeleting(true);
    deleteUserMutation.mutate(user.id, {
      onSuccess: () => {
        toast.success("User deleted successfully");
        setIsDeleting(false);
      },
      onError: () => {
        toast.error("Failed to delete user");
        setIsDeleting(false);
      },
    });
  };

  const handleRestoreUser = () => {
    setIsDeleting(true);
    updateUserMutation.mutate(
      { userId: user.id, status: "active" },
      {
        onSuccess: () => {
          toast.success("User restored successfully");
          setIsDeleting(false);
        },
        onError: () => {
          toast.error("Failed to restore user");
          setIsDeleting(false);
        },
      },
    );
  };

  const isDeleted = user.status === "deleted";

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="sm" disabled={isDeleting}>
            {isDeleting ? (
              <Loader2 className="animate-spin" />
            ) : (
              <MoreHorizontal />
            )}
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          {isDeleted ? (
            <DropdownMenuItem
              className="cursor-pointer"
              onClick={handleRestoreUser}
              disabled={isDeleting}
            >
              <RotateCcw className="text-green-600" />
              Restore User
            </DropdownMenuItem>
          ) : (
            <DropdownMenuItem
              className="cursor-pointer"
              onClick={handleOpenDeleteDialog}
              disabled={isDeleting}
            >
              <Trash2 className="text-destructive" />
              Delete User
            </DropdownMenuItem>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      <AlertDialog open={isDeleteAlertOpen} onOpenChange={setIsDeleteAlertOpen}>
        <AlertDialogContent className="sm:max-w-md bg-neutral-900">
          <AlertDialogHeader>
            <AlertDialogTitle>Delete User</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to delete user{" "}
              <span className="font-semibold">{user.name}</span>? This action
              cannot be undone. If the user is currently active, they will be
              logged out immediately.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={onConfirmDelete}
              className={buttonVariants({ variant: "destructive" })}
            >
              {deleteUserMutation.isPending ? (
                <Loader2 className="animate-spin" />
              ) : (
                "Delete"
              )}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
};

export default Actions;

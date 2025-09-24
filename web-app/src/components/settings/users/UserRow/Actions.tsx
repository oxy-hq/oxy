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
import {
  useRemoveUser,
  useUpdateUserRole,
} from "@/hooks/api/users/useUserMutations";
import useCurrentUser from "@/hooks/api/users/useCurrentUser";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/shadcn/select";

interface Props {
  user: UserInfo;
  workspaceId: string;
}

const Actions: React.FC<Props> = ({ user, workspaceId }) => {
  const { data: currentUser } = useCurrentUser();

  const [isDeleting, setIsDeleting] = useState(false);
  const [isRoleDialogOpen, setIsRoleDialogOpen] = useState(false);
  const [selectedRole, setSelectedRole] = useState(user.role);
  const removeUserMutation = useRemoveUser(workspaceId);
  const updateUserRoleMutation = useUpdateUserRole(workspaceId);
  const [isRemoveUserAlertOpen, setIsRemoveUserAlertOpen] = useState(false);
  const roleOptions = ["owner", "admin", "member"];

  if (currentUser?.id === user.id) return null;

  const handleOpenDeleteDialog = () => {
    setIsRemoveUserAlertOpen(true);
  };

  const onConfirmDelete = () => {
    setIsDeleting(true);
    removeUserMutation.mutate(user.id, {
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

  const handleOpenRoleDialog = () => {
    setSelectedRole(user.role);
    setIsRoleDialogOpen(true);
  };

  const onConfirmRoleUpdate = () => {
    updateUserRoleMutation.mutate(
      { userId: user.id, role: selectedRole },
      {
        onSuccess: () => {
          toast.success("User role updated successfully");
          setIsRoleDialogOpen(false);
        },
        onError: () => {
          toast.error("Failed to update user role");
        },
      },
    );
  };

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
          <DropdownMenuItem
            className="cursor-pointer"
            onClick={handleOpenDeleteDialog}
          >
            <Trash2 className="text-destructive" />
            Remove User
          </DropdownMenuItem>
          <DropdownMenuItem
            className="cursor-pointer"
            onClick={handleOpenRoleDialog}
          >
            <RotateCcw className="mr-2" />
            Change Role
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <AlertDialog
        open={isRemoveUserAlertOpen}
        onOpenChange={setIsRemoveUserAlertOpen}
      >
        <AlertDialogContent className="sm:max-w-md bg-neutral-900">
          <AlertDialogHeader>
            <AlertDialogTitle>Remove User from Workspace</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to remove user{" "}
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
              {removeUserMutation.isPending ? (
                <Loader2 className="animate-spin" />
              ) : (
                "Remove"
              )}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      <AlertDialog open={isRoleDialogOpen} onOpenChange={setIsRoleDialogOpen}>
        <AlertDialogContent className="sm:max-w-md bg-neutral-900">
          <AlertDialogHeader>
            <AlertDialogTitle>Change User Role</AlertDialogTitle>
            <AlertDialogDescription>
              Select a new role for{" "}
              <span className="font-semibold">{user.name}</span>.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <div className="my-4">
            <Select
              value={selectedRole}
              onValueChange={(value) =>
                setSelectedRole(value as typeof user.role)
              }
              disabled={user.role === "owner"}
            >
              <SelectTrigger id="role">
                <SelectValue placeholder="Select role" />
              </SelectTrigger>
              <SelectContent>
                {roleOptions.map((role) => (
                  <SelectItem
                    key={role}
                    value={role}
                    disabled={role === "owner" && user.role !== "owner"}
                  >
                    {role.charAt(0).toUpperCase() + role.slice(1)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={onConfirmRoleUpdate}
              className={buttonVariants({ variant: "default" })}
              disabled={
                selectedRole === user.role || updateUserRoleMutation.isPending
              }
            >
              {updateUserRoleMutation.isPending ? (
                <Loader2 className="animate-spin" />
              ) : (
                "Update Role"
              )}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
};

export default Actions;

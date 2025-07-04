import React, { memo, useState } from "react";
import { TableCell, TableRow } from "@/components/ui/shadcn/table";
import { Badge } from "@/components/ui/shadcn/badge";
import { UserActions } from "../UserActions";
import { UserInfo } from "@/types/auth";
import { capitalize } from "@/libs/utils/string";
import {
  useDeleteUser,
  useUpdateUser,
} from "@/hooks/api/users/useUserMutations";
import { toast } from "sonner";

interface UserTableRowProps {
  user: UserInfo;
  isAdmin: boolean;
}

export const UserTableRow: React.FC<UserTableRowProps> = memo(
  ({ user, isAdmin }) => {
    const [isDeleting, setIsDeleting] = useState(false);
    const deleteUserMutation = useDeleteUser();
    const updateUserMutation = useUpdateUser();

    const getRoleBadgeVariant = (role: string) => {
      return role === "admin" ? "destructive" : "secondary";
    };

    const getStatusBadgeVariant = (status: string) => {
      return status === "active" ? "default" : "outline";
    };

    const handleOpenDeleteDialog = (user: UserInfo) => {
      if (window.confirm(`Are you sure you want to delete ${user.name}?`)) {
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
      }
    };

    const handleRestoreUser = (user: UserInfo) => {
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

    return (
      <TableRow key={user.id}>
        <TableCell className="flex items-center space-x-3">
          {user.picture ? (
            <img
              src={user.picture}
              alt={user.name}
              className="w-8 h-8 rounded-full object-cover"
            />
          ) : (
            <div className="w-8 h-8 bg-gray-300 rounded-full flex items-center justify-center text-sm">
              {user.name.charAt(0).toUpperCase()}
            </div>
          )}
          <span className="font-medium">{user.name}</span>
        </TableCell>
        <TableCell className="text-muted-foreground">{user.email}</TableCell>
        <TableCell>
          <Badge variant={getRoleBadgeVariant(user.role)}>
            {capitalize(user.role)}
          </Badge>
        </TableCell>
        <TableCell>
          <Badge variant={getStatusBadgeVariant(user.status)}>
            {capitalize(user.status)}
          </Badge>
        </TableCell>
        {isAdmin && (
          <TableCell>
            <UserActions
              user={user}
              isLoading={isDeleting}
              onOpenDeleteDialog={handleOpenDeleteDialog}
              onRestoreUser={handleRestoreUser}
            />
          </TableCell>
        )}
      </TableRow>
    );
  },
);

UserTableRow.displayName = "UserTableRow";

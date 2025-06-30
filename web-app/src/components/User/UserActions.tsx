import React, { memo } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/shadcn/dropdown-menu";
import { MoreHorizontal, Trash2, Loader2, RotateCcw } from "lucide-react";
import { UserInfo } from "@/types/auth";

interface UserActionsProps {
  user: UserInfo;
  isLoading: boolean;
  onOpenDeleteDialog: (user: UserInfo) => void;
  onRestoreUser?: (user: UserInfo) => void;
}

export const UserActions: React.FC<UserActionsProps> = memo(
  ({ user, isLoading, onOpenDeleteDialog, onRestoreUser }) => {
    const isDeleted = user.status === "deleted";

    return (
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="h-8 w-8 p-0"
            disabled={isLoading}
          >
            {isLoading ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <MoreHorizontal className="h-4 w-4" />
            )}
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          {isDeleted ? (
            <DropdownMenuItem
              onClick={() => onRestoreUser?.(user)}
              disabled={isLoading}
              className="text-green-600 focus:text-green-600"
            >
              <RotateCcw className="h-4 w-4 mr-2" />
              Restore User
            </DropdownMenuItem>
          ) : (
            <DropdownMenuItem
              onClick={() => onOpenDeleteDialog(user)}
              disabled={isLoading}
              className="text-destructive focus:text-destructive"
            >
              <Trash2 className="h-4 w-4 mr-2" />
              Delete User
            </DropdownMenuItem>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
    );
  },
);

UserActions.displayName = "UserActions";

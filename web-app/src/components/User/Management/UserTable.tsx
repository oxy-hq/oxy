import React, { memo, useCallback } from "react";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/shadcn/table";
import { UserTableRow } from "./UserTableRow";
import { UserInfo } from "@/types/auth";
import { useAuth } from "@/contexts/AuthContext";
import useCurrentUser from "@/hooks/api/useCurrentUser";

interface UserTableProps {
  users: UserInfo[];
  error: Error | null;
}

export const UserTable: React.FC<UserTableProps> = memo(({ users, error }) => {
  const { getUser } = useAuth();
  const { data: currentUser } = useCurrentUser();

  const isAdmin = useCallback((): boolean => {
    // First try to get role from the API
    if (currentUser?.role) {
      return currentUser.role === "admin";
    }

    // Fallback to localStorage
    const userStr = getUser();
    if (userStr) {
      const parsed = JSON.parse(userStr);
      return parsed?.role === "admin";
    }
    return false;
  }, [currentUser, getUser]);

  const adminUser = isAdmin();

  return (
    <div className="space-y-4">
      {error && (
        <div className="bg-destructive/15 text-destructive px-4 py-3 rounded-lg">
          <p className="font-medium">Error loading users</p>
          <p className="text-sm mt-1">{error.message}</p>
        </div>
      )}

      <div className="border rounded-lg">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>User</TableHead>
              <TableHead>Email</TableHead>
              <TableHead>Role</TableHead>
              <TableHead>Status</TableHead>
              {adminUser && <TableHead className="w-24">Actions</TableHead>}
            </TableRow>
          </TableHeader>
          <TableBody>
            {" "}
            {users.map((user) => (
              <UserTableRow key={user.id} user={user} isAdmin={adminUser} />
            ))}
            {users.length === 0 && (
              <TableRow>
                <TableCell
                  colSpan={adminUser ? 5 : 4}
                  className="text-center py-8 text-muted-foreground"
                >
                  {error ? "Unable to load users" : "No users found"}
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </div>
    </div>
  );
});

UserTable.displayName = "UserTable";

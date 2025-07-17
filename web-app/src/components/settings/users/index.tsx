import React, { useCallback } from "react";
import useUsers from "@/hooks/api/users/useUsers";
import PageWrapper from "../components/PageWrapper";
import {
  Table,
  TableBody,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/shadcn/table";
import TableContentWrapper from "../components/TableContentWrapper";
import { useAuth } from "@/contexts/AuthContext";
import useCurrentUser from "@/hooks/api/users/useCurrentUser";
import UserRow from "./UserRow";
import TableWrapper from "../components/TableWrapper";

const UserManagement: React.FC = () => {
  const { getUser } = useAuth();
  const { data: usersData, isLoading: loading, error } = useUsers();
  const users = usersData?.users || [];

  const { data: currentUser } = useCurrentUser();

  const isAdmin = useCallback((): boolean => {
    if (currentUser?.role) {
      return currentUser.role === "admin";
    }
    const userStr = getUser();
    if (userStr) {
      const parsed = JSON.parse(userStr);
      return parsed?.role === "admin";
    }
    return false;
  }, [currentUser, getUser]);

  const adminUser = isAdmin();

  return (
    <PageWrapper title="Users">
      <TableWrapper>
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
            <TableContentWrapper
              isEmpty={users.length === 0}
              loading={loading}
              colSpan={adminUser ? 5 : 4}
              error={error?.message}
              noFoundTitle="No users found"
              noFoundDescription="There are currently no users in the system."
            >
              {users.map((user) => (
                <UserRow key={user.id} user={user} isAdmin={adminUser} />
              ))}
            </TableContentWrapper>
          </TableBody>
        </Table>
      </TableWrapper>
    </PageWrapper>
  );
};

export default UserManagement;

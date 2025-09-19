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
import useCurrentUser from "@/hooks/api/users/useCurrentUser";
import UserRow from "./UserRow";
import TableWrapper from "../components/TableWrapper";
import useCurrentProject from "@/stores/useCurrentProject";
import AddMemberForm from "./AddMemberForm";

const UserManagement: React.FC = () => {
  const { project } = useCurrentProject();

  if (!project) {
    throw new Error("No project selected");
  }

  const { organization_id } = project;
  const {
    data: usersData,
    isLoading: loading,
    error,
  } = useUsers(organization_id);
  const users = usersData?.users || [];

  const { data: currentUser } = useCurrentUser();

  const isAdmin = useCallback((): boolean => {
    if (!currentUser) {
      return false;
    }
    if (!usersData) {
      return false;
    }
    for (const user of usersData.users) {
      if (
        user.id === currentUser?.id &&
        (user.role === "admin" || user.role === "owner")
      ) {
        return true;
      }
    }
    return false;
  }, [currentUser, usersData]);

  const adminUser = isAdmin();

  return (
    <PageWrapper
      title="Users"
      actions={<AddMemberForm organizationId={organization_id} />}
    >
      <TableWrapper>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>User</TableHead>
              <TableHead>Email</TableHead>
              <TableHead>Role</TableHead>
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
                <UserRow
                  key={user.id}
                  user={user}
                  organizationId={organization_id}
                  isAdmin={adminUser}
                />
              ))}
            </TableContentWrapper>
          </TableBody>
        </Table>
      </TableWrapper>
    </PageWrapper>
  );
};

export default UserManagement;

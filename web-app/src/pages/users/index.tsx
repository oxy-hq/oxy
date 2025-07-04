import React from "react";
import useUsers from "@/hooks/api/users/useUsers";
import {
  UserManagementHeader,
  UserTable,
  UserManagementLoadingState,
} from "@/components/User/Management";

const UserManagement: React.FC = () => {
  const { data: usersData, isLoading: loading, error } = useUsers();
  const users = usersData?.users || [];

  if (loading && !error) {
    return <UserManagementLoadingState />;
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 p-6">
        <div className="max-w-4xl mx-auto">
          <UserManagementHeader />
          <UserTable users={users} error={error} />
        </div>
      </div>
    </div>
  );
};

export default UserManagement;

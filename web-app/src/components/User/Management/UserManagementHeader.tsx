import React from "react";
import { Users } from "lucide-react";

export const UserManagementHeader: React.FC = () => {
  return (
    <div className="flex items-center space-x-3 mb-6">
      <Users className="h-6 w-6" />
      <div>
        <h1 className="text-xl font-semibold">Users</h1>
        <p className="text-sm text-muted-foreground">
          Manage user roles and permissions
        </p>
      </div>
    </div>
  );
};

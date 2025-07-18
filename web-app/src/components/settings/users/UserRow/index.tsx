import React from "react";
import { TableCell, TableRow } from "@/components/ui/shadcn/table";
import { Badge } from "@/components/ui/shadcn/badge";
import {
  Avatar,
  AvatarFallback,
  AvatarImage,
} from "@/components/ui/shadcn/avatar";
import Actions from "./Actions";
import { UserInfo } from "@/types/auth";
import { capitalize } from "@/libs/utils/string";
import useCurrentUser from "@/hooks/api/users/useCurrentUser";

interface Props {
  user: UserInfo;
  isAdmin: boolean;
}

const UserRow: React.FC<Props> = ({ user, isAdmin }) => {
  const { data: currentUser } = useCurrentUser();
  const getRoleBadgeVariant = (role: string) => {
    return role === "admin" ? "destructive" : "secondary";
  };

  const getStatusBadgeVariant = (status: string) => {
    return status === "active" ? "default" : "outline";
  };

  return (
    <TableRow key={user.id}>
      <TableCell className="flex items-center space-x-3">
        <Avatar>
          <AvatarImage src={user.picture} alt={user.name} />
          <AvatarFallback>{user.name.charAt(0).toUpperCase()}</AvatarFallback>
        </Avatar>
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
      {isAdmin && user.id !== currentUser?.id && (
        <TableCell>
          <Actions user={user} />
        </TableCell>
      )}
    </TableRow>
  );
};

export default UserRow;

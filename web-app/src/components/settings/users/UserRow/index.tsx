import type React from "react";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/shadcn/avatar";
import { Badge } from "@/components/ui/shadcn/badge";
import { TableCell, TableRow } from "@/components/ui/shadcn/table";
import useCurrentUser from "@/hooks/api/users/useCurrentUser";
import { capitalize } from "@/libs/utils/string";
import type { UserInfo } from "@/types/auth";
import Actions from "./Actions";

interface Props {
  user: UserInfo;
  isAdmin: boolean;
  workspaceId: string;
}

const UserRow: React.FC<Props> = ({ user, isAdmin, workspaceId }) => {
  const { data: currentUser } = useCurrentUser();
  const getRoleBadgeVariant = (role: string) => {
    return role === "admin" ? "destructive" : "secondary";
  };

  return (
    <TableRow key={user.id}>
      <TableCell className='flex items-center space-x-3'>
        <Avatar>
          <AvatarImage src={user.picture} alt={user.name} />
          <AvatarFallback>{user.name.charAt(0).toUpperCase()}</AvatarFallback>
        </Avatar>
        <span className='font-medium'>{user.name}</span>
      </TableCell>
      <TableCell className='text-muted-foreground'>{user.email}</TableCell>
      <TableCell>
        <Badge variant={getRoleBadgeVariant(user.role)}>{capitalize(user.role)}</Badge>
      </TableCell>
      {isAdmin && user.id !== currentUser?.id && (
        <TableCell>
          <Actions user={user} workspaceId={workspaceId} />
        </TableCell>
      )}
    </TableRow>
  );
};

export default UserRow;

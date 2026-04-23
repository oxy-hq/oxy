import { Loader2, MoreHorizontal } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from "@/components/ui/shadcn/alert-dialog";
import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";
import { useRemoveMember, useUpdateMemberRole } from "@/hooks/api/organizations";
import type { OrgMember, OrgRole } from "@/types/organization";

export function MemberRowActions({
  member,
  orgId,
  viewerRole,
  isSelf,
  ownerCount
}: {
  member: OrgMember;
  orgId: string;
  viewerRole: OrgRole;
  isSelf: boolean;
  ownerCount: number;
}) {
  const [deleteOpen, setDeleteOpen] = useState(false);
  const updateRole = useUpdateMemberRole();
  const removeMember = useRemoveMember();
  const displayName = member.name || member.email.split("@")[0];
  const isLastOwner = member.role === "owner" && ownerCount <= 1;

  if (isSelf || isLastOwner) return null;

  const canManage = viewerRole === "owner" || viewerRole === "admin";
  if (!canManage) return null;

  const canGrantAdmin = viewerRole === "owner" && member.role === "member";
  const canGrantOwner = viewerRole === "owner" && member.role !== "owner";
  const canRevokeAdmin =
    (viewerRole === "owner" || viewerRole === "admin") && member.role === "admin";
  const canDemoteToMember = viewerRole === "owner" && member.role !== "member";

  const handleRoleChange = (role: string) => {
    updateRole.mutate(
      { orgId, userId: member.user_id, role },
      { onError: () => toast.error("Failed to update role") }
    );
  };

  const handleRemove = () => {
    removeMember.mutate(
      { orgId, userId: member.user_id },
      {
        onSuccess: () => toast.success(`${displayName} removed`),
        onError: () => toast.error("Failed to remove member")
      }
    );
  };

  const isPending = updateRole.isPending || removeMember.isPending;

  return (
    <>
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger asChild>
          <Button
            variant='ghost'
            size='icon'
            className='h-8 w-8 data-[state=open]:bg-muted'
            disabled={isPending}
          >
            {isPending ? (
              <Loader2 className='h-4 w-4 animate-spin' />
            ) : (
              <MoreHorizontal className='h-4 w-4' />
            )}
            <span className='sr-only'>Open menu</span>
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align='end' className='w-44'>
          {canGrantOwner && member.role !== "owner" && (
            <DropdownMenuItem onClick={() => handleRoleChange("owner")}>
              Make owner
            </DropdownMenuItem>
          )}
          {canGrantAdmin && (
            <DropdownMenuItem onClick={() => handleRoleChange("admin")}>
              Grant admin
            </DropdownMenuItem>
          )}
          {canRevokeAdmin && (
            <DropdownMenuItem onClick={() => handleRoleChange("member")}>
              Revoke admin
            </DropdownMenuItem>
          )}
          {canDemoteToMember && member.role !== "member" && !canRevokeAdmin && (
            <DropdownMenuItem onClick={() => handleRoleChange("member")}>
              Demote to member
            </DropdownMenuItem>
          )}
          <DropdownMenuSeparator />
          <DropdownMenuItem
            className='text-destructive focus:text-destructive'
            onClick={() => setDeleteOpen(true)}
          >
            Remove
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Remove {displayName}?</AlertDialogTitle>
            <AlertDialogDescription>
              This will revoke their access. They can be re-invited later.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              className='bg-destructive text-destructive-foreground hover:bg-destructive/90'
              onClick={handleRemove}
            >
              Remove
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

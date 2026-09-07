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
import { useDeleteRole } from "@/hooks/api/organizations";
import { apiErrorMessage, apiStatus } from "@/libs/apiError";
import type { RoleRow } from "@/types/operatingGraph";
import { heldByLabel } from "../utils";
import { RenamePositionDialog } from "./RenamePositionDialog";

/**
 * Rename and Delete. Delete asks first, and the server refuses it with a 409
 * while anyone holds the position — the toast says so and names the fix,
 * rather than the menu hiding the item and leaving the admin guessing.
 */
export function PositionRowActions({
  orgId,
  role,
  holders
}: {
  orgId: string;
  role: RoleRow;
  holders: number;
}) {
  const [renameOpen, setRenameOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const deleteRole = useDeleteRole();

  const handleDelete = () => {
    deleteRole.mutate(
      { orgId, roleId: role.id },
      {
        onSuccess: () => {
          toast.success(`Deleted ${role.name}`);
          setDeleteOpen(false);
        },
        onError: (err) => {
          setDeleteOpen(false);
          if (apiStatus(err) === 409) {
            toast.error(
              `${role.name} is still held by ${heldByLabel(holders).toLowerCase()}. Remove those assignments first.`
            );
            return;
          }
          toast.error(apiErrorMessage(err, "Couldn't delete the position"));
        }
      }
    );
  };

  return (
    <>
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger asChild>
          <Button
            variant='ghost'
            size='icon'
            className='h-8 w-8 data-[state=open]:bg-muted'
            disabled={deleteRole.isPending}
            data-testid={`settings-positions-menu-${role.id}`}
          >
            {deleteRole.isPending ? (
              <Loader2 className='h-4 w-4 animate-spin' />
            ) : (
              <MoreHorizontal className='h-4 w-4' />
            )}
            <span className='sr-only'>Open menu</span>
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align='end' className='w-40'>
          <DropdownMenuItem
            onClick={() => setRenameOpen(true)}
            data-testid='settings-positions-rename'
          >
            Rename…
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            className='text-destructive focus:text-destructive'
            onClick={() => setDeleteOpen(true)}
            data-testid='settings-positions-delete'
          >
            Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <RenamePositionDialog
        open={renameOpen}
        onOpenChange={setRenameOpen}
        orgId={orgId}
        role={role}
      />

      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete {role.name}?</AlertDialogTitle>
            <AlertDialogDescription>
              {holders > 0
                ? `It is held by ${heldByLabel(holders)}. The server refuses to delete a position anyone holds, so remove those assignments first.`
                : "Nobody holds it, so nothing else changes. You can create it again later."}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              className='bg-destructive text-destructive-foreground hover:bg-destructive/90'
              onClick={(e) => {
                e.preventDefault();
                handleDelete();
              }}
              disabled={deleteRole.isPending}
              data-testid='settings-positions-delete-confirm'
            >
              {deleteRole.isPending && <Loader2 className='h-4 w-4 animate-spin' />}
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

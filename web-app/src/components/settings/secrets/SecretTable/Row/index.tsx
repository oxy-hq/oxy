import { formatDistanceToNow } from "date-fns";
import { Edit, Trash2 } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { toast } from "sonner";
import { useMediaQuery } from "usehooks-ts";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { TableCell, TableRow } from "@/components/ui/shadcn/table";
import { useDeleteSecret } from "@/hooks/api/secrets/useSecretMutations";
import type { Secret } from "@/types/secret";
import { DeleteSecretDialog } from "./DeleteSecretDialog";
import { EditSecretDialog } from "./EditSecretDialog";

interface Props {
  secret: Secret;
}

export const SecretRow: React.FC<Props> = ({ secret }) => {
  const [isEditDialogOpen, setIsEditDialogOpen] = useState(false);
  const [isDeleteDialogOpen, setIsDeleteDialogOpen] = useState(false);
  const deleteSecretMutation = useDeleteSecret();

  const isMobile = useMediaQuery("(max-width: 767px)");

  const handleDeleteSecret = async () => {
    await deleteSecretMutation.mutateAsync(secret.id);
    setIsDeleteDialogOpen(false);
  };

  const handleSecretUpdated = () => {
    toast.success("Secret updated successfully");
  };

  return (
    <TableRow key={secret.id}>
      <TableCell className='font-medium'>
        <div className='flex flex-col gap-1'>
          <span>{secret.name}</span>
          {isMobile && secret.description && (
            <span className='mt-1 text-muted-foreground text-xs'>{secret.description}</span>
          )}
        </div>
      </TableCell>
      {!isMobile && (
        <TableCell>
          {secret.description || (
            <span className='text-muted-foreground italic'>No description</span>
          )}
        </TableCell>
      )}
      <TableCell>
        <Badge variant={secret.is_active ? "default" : "secondary"}>
          {secret.is_active ? "Active" : "Inactive"}
        </Badge>
      </TableCell>
      {!isMobile && (
        <TableCell>
          {formatDistanceToNow(new Date(secret.created_at), {
            addSuffix: true
          })}
        </TableCell>
      )}
      {!isMobile && (
        <TableCell>
          {formatDistanceToNow(new Date(secret.updated_at), {
            addSuffix: true
          })}
        </TableCell>
      )}
      <TableCell>
        <div className='flex items-center justify-center gap-2'>
          <Button
            variant='ghost'
            size='sm'
            onClick={() => setIsEditDialogOpen(true)}
            title='Edit secret'
          >
            <Edit />
          </Button>
          <Button
            variant='ghost'
            size='sm'
            onClick={() => setIsDeleteDialogOpen(true)}
            title='Delete secret'
          >
            <Trash2 className='!text-destructive' />
          </Button>
        </div>
      </TableCell>
      <EditSecretDialog
        open={isEditDialogOpen}
        onOpenChange={setIsEditDialogOpen}
        secret={secret}
        onSecretUpdated={handleSecretUpdated}
      />

      <DeleteSecretDialog
        open={isDeleteDialogOpen}
        onOpenChange={setIsDeleteDialogOpen}
        secret={secret}
        onConfirm={handleDeleteSecret}
      />
    </TableRow>
  );
};

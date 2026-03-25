import { formatDistanceToNow } from "date-fns";
import { Edit, Trash2 } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { toast } from "sonner";
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

  const handleDeleteSecret = async () => {
    await deleteSecretMutation.mutateAsync(secret.id);
    setIsDeleteDialogOpen(false);
  };

  const handleSecretUpdated = () => {
    toast.success("Secret updated successfully");
  };

  return (
    <TableRow>
      <TableCell className='w-full max-w-0'>
        <div className='truncate font-medium'>{secret.name}</div>
        {secret.description && (
          <div className='truncate font-mono text-muted-foreground text-sm'>
            {secret.description}
          </div>
        )}
      </TableCell>
      <TableCell className='whitespace-nowrap text-muted-foreground text-sm'>
        {formatDistanceToNow(new Date(secret.created_at), { addSuffix: true })}
      </TableCell>
      <TableCell className='w-px whitespace-nowrap'>
        <div className='flex items-center gap-1'>
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

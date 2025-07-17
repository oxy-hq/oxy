import React, { useState } from "react";
import { TableCell, TableRow } from "@/components/ui/shadcn/table";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { Edit, Trash2 } from "lucide-react";
import { Secret } from "@/types/secret";
import { formatDistanceToNow } from "date-fns";
import { EditSecretDialog } from "./EditSecretDialog";
import { DeleteSecretDialog } from "./DeleteSecretDialog";
import { useDeleteSecret } from "@/hooks/api/useSecretMutations";
import { useMediaQuery } from "usehooks-ts";
import { toast } from "sonner";

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
      <TableCell className="font-medium">
        <div className="flex flex-col gap-1">
          <span>{secret.name}</span>
          {isMobile && secret.description && (
            <span className="text-xs text-muted-foreground mt-1">
              {secret.description}
            </span>
          )}
        </div>
      </TableCell>
      {!isMobile && (
        <TableCell>
          {secret.description || (
            <span className="text-muted-foreground italic">No description</span>
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
            addSuffix: true,
          })}
        </TableCell>
      )}
      {!isMobile && (
        <TableCell>
          {formatDistanceToNow(new Date(secret.updated_at), {
            addSuffix: true,
          })}
        </TableCell>
      )}
      <TableCell>
        <div className="flex items-center justify-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setIsEditDialogOpen(true)}
            title="Edit secret"
          >
            <Edit />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setIsDeleteDialogOpen(true)}
            title="Delete secret"
          >
            <Trash2 className="!text-destructive" />
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

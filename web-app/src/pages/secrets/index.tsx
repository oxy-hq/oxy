import React, { useState } from "react";
import { Plus } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { toast } from "sonner";
import { Secret } from "@/types/secret";
import PageHeader from "@/components/PageHeader";
import { useSidebar } from "@/components/ui/shadcn/sidebar";
import { SecretTable } from "./components";
import { CreateSecretDialog } from "./components";
import { EditSecretDialog } from "./components";
import { DeleteSecretDialog } from "./components";
import useSecrets from "@/hooks/api/useSecrets";
import { useDeleteSecret } from "@/hooks/api/useSecretMutations";

const SecretManagement: React.FC = () => {
  const { open } = useSidebar();
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [isEditDialogOpen, setIsEditDialogOpen] = useState(false);
  const [isDeleteDialogOpen, setIsDeleteDialogOpen] = useState(false);
  const [selectedSecret, setSelectedSecret] = useState<Secret | null>(null);

  // Use the secrets hook
  const { data: secretsResponse, isLoading: loading } = useSecrets();
  const secrets = secretsResponse?.secrets || [];

  // Use mutation hooks
  const deleteSecretMutation = useDeleteSecret();

  const handleDeleteSecret = async () => {
    if (!selectedSecret) return;

    await deleteSecretMutation.mutateAsync(selectedSecret.id);
    setIsDeleteDialogOpen(false);
    setSelectedSecret(null);
  };

  const handleSecretCreated = () => {
    toast.success("Secret created successfully");
    setIsCreateDialogOpen(false);
  };

  const handleSecretUpdated = () => {
    toast.success("Secret updated successfully");
    setIsEditDialogOpen(false);
    setSelectedSecret(null);
  };

  const openEditDialog = (secret: Secret) => {
    setSelectedSecret(secret);
    setIsEditDialogOpen(true);
  };

  const openDeleteDialog = (secret: Secret) => {
    setSelectedSecret(secret);
    setIsDeleteDialogOpen(true);
  };

  return (
    <div className="flex flex-col h-full">
      {!open && <PageHeader />}

      <div className="flex-1 p-6">
        <div className="max-w-6xl mx-auto">
          {/* Header */}
          <div className="flex justify-between items-center mb-6">
            <div>
              <h1 className="text-2xl font-semibold">Secrets</h1>
            </div>
            <Button onClick={() => setIsCreateDialogOpen(true)}>
              <Plus className="w-4 h-4 mr-1" />
              Create
            </Button>
          </div>

          {/* Secrets Table */}
          <SecretTable
            secrets={secrets}
            loading={loading}
            onEditClick={openEditDialog}
            onDeleteClick={openDeleteDialog}
          />
        </div>
      </div>

      {/* Create Secret Dialog */}
      <CreateSecretDialog
        open={isCreateDialogOpen}
        onOpenChange={setIsCreateDialogOpen}
        onSecretCreated={handleSecretCreated}
      />

      {/* Edit Secret Dialog */}
      <EditSecretDialog
        open={isEditDialogOpen}
        onOpenChange={setIsEditDialogOpen}
        secret={selectedSecret}
        onSecretUpdated={handleSecretUpdated}
      />

      {/* Delete Confirmation Dialog */}
      <DeleteSecretDialog
        open={isDeleteDialogOpen}
        onOpenChange={setIsDeleteDialogOpen}
        secret={selectedSecret}
        onConfirm={handleDeleteSecret}
      />
    </div>
  );
};

export default SecretManagement;

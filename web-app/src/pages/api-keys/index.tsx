import React, { useState } from "react";
import { Plus } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { toast } from "sonner";
import { ApiKey, CreateApiKeyResponse } from "@/types/apiKey";
import PageHeader from "@/components/PageHeader";
import { useSidebar } from "@/components/ui/shadcn/sidebar";
import { ApiKeyTable } from "./components/ApiKeyTable";
import { CreateApiKeyDialog } from "./components/CreateApiKeyDialog";
import { DeleteApiKeyDialog } from "./components/DeleteApiKeyDialog";
import { NewApiKeyBanner } from "./components/NewApiKeyBanner";
import useApiKeys from "@/hooks/api/useApiKeys";
import { useRevokeApiKey } from "@/hooks/api/useApiKeyMutations";

const ApiKeyManagement: React.FC = () => {
  const { open } = useSidebar();
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [isDeleteDialogOpen, setIsDeleteDialogOpen] = useState(false);
  const [selectedApiKey, setSelectedApiKey] = useState<ApiKey | null>(null);
  const [newApiKey, setNewApiKey] = useState<CreateApiKeyResponse | null>(null);

  // Use the API keys hook
  const { data: apiKeysResponse, isLoading: loading } = useApiKeys();
  const apiKeys = apiKeysResponse?.api_keys || [];

  // Use mutation hooks
  const revokeApiKeyMutation = useRevokeApiKey();

  const handleDeleteApiKey = async () => {
    if (!selectedApiKey) return;

    await revokeApiKeyMutation.mutateAsync(selectedApiKey.id);
    setIsDeleteDialogOpen(false);
    setSelectedApiKey(null);
  };

  const handleApiKeyCreated = (apiKey: CreateApiKeyResponse) => {
    setNewApiKey(apiKey);
  };

  const copyToClipboard = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toast.success("Copied to clipboard");
    } catch (error) {
      console.error("Failed to copy to clipboard:", error);
      toast.error("Failed to copy to clipboard");
    }
  };

  const openDeleteDialog = (apiKey: ApiKey) => {
    setSelectedApiKey(apiKey);
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
              <h1 className="text-2xl font-semibold">API Key Management</h1>
              <p className="text-muted-foreground mt-1">
                Create and manage API keys for programmatic access
              </p>
            </div>
            <Button onClick={() => setIsCreateDialogOpen(true)}>
              <Plus className="w-4 h-4 mr-2" />
              Create API Key
            </Button>
          </div>

          {/* New API Key Display */}
          {newApiKey && (
            <NewApiKeyBanner
              apiKey={newApiKey}
              onDismiss={() => setNewApiKey(null)}
              onCopy={copyToClipboard}
            />
          )}

          {/* API Keys Table */}
          <ApiKeyTable
            apiKeys={apiKeys}
            loading={loading}
            onDeleteClick={openDeleteDialog}
          />
        </div>
      </div>

      {/* Create API Key Dialog */}
      <CreateApiKeyDialog
        open={isCreateDialogOpen}
        onOpenChange={setIsCreateDialogOpen}
        onApiKeyCreated={handleApiKeyCreated}
      />

      {/* Delete Confirmation Dialog */}
      <DeleteApiKeyDialog
        open={isDeleteDialogOpen}
        onOpenChange={setIsDeleteDialogOpen}
        apiKey={selectedApiKey}
        onConfirm={handleDeleteApiKey}
      />
    </div>
  );
};

export default ApiKeyManagement;

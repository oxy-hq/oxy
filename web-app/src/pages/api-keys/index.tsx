import React, { useState } from "react";
import { Plus, Key } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { toast } from "sonner";
import { ApiKey, CreateApiKeyResponse } from "@/types/apiKey";
import { ApiKeyTable } from "./components/ApiKeyTable";
import { CreateApiKeyDialog } from "./components/CreateApiKeyDialog";
import { DeleteApiKeyDialog } from "./components/DeleteApiKeyDialog";
import { NewApiKeyBanner } from "./components/NewApiKeyBanner";
import useApiKeys from "@/hooks/api/apiKeys/useApiKeys";
import { useRevokeApiKey } from "@/hooks/api/apiKeys/useApiKeyMutations";

const ApiKeyManagement: React.FC = () => {
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

  if (loading) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex-1 p-6">
          <div className="max-w-4xl mx-auto">
            <div className="flex items-center justify-center h-64">
              <div className="text-lg">Loading API keys...</div>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 p-6">
        <div className="max-w-4xl mx-auto">
          {/* Header */}
          <div className="flex items-center justify-between mb-6">
            <div className="flex items-center space-x-3">
              <Key className="h-6 w-6" />
              <div>
                <h1 className="text-xl font-semibold">API Keys</h1>
                <p className="text-sm text-muted-foreground">
                  Create and manage API keys for programmatic access
                </p>
              </div>
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
          <div className="border rounded-lg">
            <ApiKeyTable
              apiKeys={apiKeys}
              loading={loading}
              onDeleteClick={openDeleteDialog}
            />
          </div>
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

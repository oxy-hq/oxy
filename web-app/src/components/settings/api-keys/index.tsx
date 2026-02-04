import type React from "react";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import type { CreateApiKeyResponse } from "@/types/apiKey";
import PageWrapper from "../components/PageWrapper";
import ApiKeyTable from "./ApiKeyTable";
import CreateApiKeyDialog from "./CreateApiKeyDialog";
import NewApiKeyBanner from "./NewApiKeyBanner";

const ApiKeyManagement: React.FC = () => {
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [newApiKey, setNewApiKey] = useState<CreateApiKeyResponse | null>(null);

  const handleApiKeyCreated = (apiKey: CreateApiKeyResponse) => {
    setNewApiKey(apiKey);
  };

  return (
    <PageWrapper
      title='API Keys'
      actions={
        <Button size='sm' onClick={() => setIsCreateDialogOpen(true)}>
          Create
        </Button>
      }
    >
      {newApiKey && <NewApiKeyBanner apiKey={newApiKey} onDismiss={() => setNewApiKey(null)} />}

      <ApiKeyTable />

      <CreateApiKeyDialog
        open={isCreateDialogOpen}
        onOpenChange={setIsCreateDialogOpen}
        onApiKeyCreated={handleApiKeyCreated}
      />
    </PageWrapper>
  );
};

export default ApiKeyManagement;

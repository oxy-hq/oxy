import { Copy, KeyRound, Plus } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import PageHeader from "@/pages/ide/components/PageHeader";
import useCurrentProject from "@/stores/useCurrentProject";
import type { CreateApiKeyResponse } from "@/types/apiKey";
import ApiKeyTable from "./ApiKeyTable";
import CreateApiKeyDialog from "./CreateApiKeyDialog";
import NewApiKeyBanner from "./NewApiKeyBanner";

const ApiKeyManagement: React.FC = () => {
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [newApiKey, setNewApiKey] = useState<CreateApiKeyResponse | null>(null);
  const { project } = useCurrentProject();

  const handleApiKeyCreated = (apiKey: CreateApiKeyResponse) => {
    setNewApiKey(apiKey);
  };

  const copyProjectId = async () => {
    if (!project?.id) return;
    try {
      await navigator.clipboard.writeText(project.id);
      toast.success("Copied to clipboard");
    } catch {
      toast.error("Failed to copy to clipboard");
    }
  };

  const listViewActions = (
    <Button size='sm' variant='outline' onClick={() => setIsCreateDialogOpen(true)}>
      <Plus />
      Create
    </Button>
  );

  return (
    <div className='flex h-full flex-col'>
      <PageHeader icon={KeyRound} title='API Keys' actions={listViewActions} />

      <div className='scrollbar-gutter-auto min-h-0 flex-1 space-y-2 overflow-auto p-4'>
        <p className='mb-2 text-muted-foreground text-sm'>Current Project ID</p>
        <div className='flex items-center gap-2'>
          <div className='flex h-8 flex-1 items-center rounded-md border bg-background px-3 font-mono text-sm'>
            {project?.id ?? "—"}
          </div>
          <Button variant='outline' size='sm' onClick={copyProjectId}>
            <Copy className='h-4 w-4' />
          </Button>
        </div>

        {newApiKey && <NewApiKeyBanner apiKey={newApiKey} onDismiss={() => setNewApiKey(null)} />}

        <ApiKeyTable />
      </div>

      <CreateApiKeyDialog
        open={isCreateDialogOpen}
        onOpenChange={setIsCreateDialogOpen}
        onApiKeyCreated={handleApiKeyCreated}
      />
    </div>
  );
};

export default ApiKeyManagement;

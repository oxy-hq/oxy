import { Copy, KeyRound, Plus } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { toast } from "sonner";
import { CanWorkspaceAdmin } from "@/components/auth/Can";
import { Button } from "@/components/ui/shadcn/button";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import type { CreateApiKeyResponse } from "@/types/apiKey";
import NoAccessNotice from "../../../components/NoAccessNotice";
import SectionHeader from "../../../components/SectionHeader";
import ApiKeyTable from "./ApiKeyTable";
import CreateApiKeyDialog from "./CreateApiKeyDialog";
import NewApiKeyBanner from "./NewApiKeyBanner";

const ApiKeys: React.FC = () => {
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [newApiKey, setNewApiKey] = useState<CreateApiKeyResponse | null>(null);
  const { workspace } = useCurrentWorkspace();

  const handleApiKeyCreated = (apiKey: CreateApiKeyResponse) => {
    setNewApiKey(apiKey);
  };

  const copyProjectId = async () => {
    if (!workspace?.id) return;
    try {
      await navigator.clipboard.writeText(workspace.id);
      toast.success("Copied to clipboard");
    } catch {
      toast.error("Failed to copy to clipboard");
    }
  };

  return (
    <CanWorkspaceAdmin
      fallback={
        <NoAccessNotice>You need workspace admin access to manage API keys.</NoAccessNotice>
      }
    >
      <div className='flex flex-col gap-5'>
        <SectionHeader
          icon={KeyRound}
          title='API Keys'
          actions={
            <Button size='sm' variant='outline' onClick={() => setIsCreateDialogOpen(true)}>
              <Plus />
              Create
            </Button>
          }
        />

        <div className='space-y-2'>
          <p className='text-muted-foreground text-sm'>Current Project ID</p>
          <div className='flex items-center gap-2'>
            <div className='flex h-8 flex-1 items-center rounded-md border bg-background px-3 font-mono text-sm'>
              {workspace?.id ?? "—"}
            </div>
            <Button variant='outline' size='sm' onClick={copyProjectId}>
              <Copy className='h-4 w-4' />
            </Button>
          </div>
        </div>

        {newApiKey && <NewApiKeyBanner apiKey={newApiKey} onDismiss={() => setNewApiKey(null)} />}

        <ApiKeyTable />

        <CreateApiKeyDialog
          open={isCreateDialogOpen}
          onOpenChange={setIsCreateDialogOpen}
          onApiKeyCreated={handleApiKeyCreated}
        />
      </div>
    </CanWorkspaceAdmin>
  );
};

export default ApiKeys;

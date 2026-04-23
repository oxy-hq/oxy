import { KeyRound, Plus } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { toast } from "sonner";
import { CanWorkspaceAdmin } from "@/components/auth/Can";
import { CreateSecretDialog } from "@/components/settings/secrets/CreateSecretDialog";
import { UnifiedSecretsTable } from "@/components/settings/secrets/UnifiedSecretsTable";
import { Button } from "@/components/ui/shadcn/button";
import PageHeader from "@/pages/ide/components/PageHeader";

const SecretsPage: React.FC = () => {
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);

  return (
    <CanWorkspaceAdmin fallback={<NoPermission label='secrets' />}>
      <div className='flex h-full flex-col'>
        <PageHeader
          icon={KeyRound}
          title='Secrets'
          actions={
            <Button size='sm' variant='outline' onClick={() => setIsCreateDialogOpen(true)}>
              <Plus />
              Create
            </Button>
          }
        />

        <div className='scrollbar-gutter-auto min-h-0 flex-1 overflow-auto p-4'>
          <UnifiedSecretsTable />
        </div>

        <CreateSecretDialog
          open={isCreateDialogOpen}
          onOpenChange={setIsCreateDialogOpen}
          onSecretCreated={() => {
            toast.success("Secret created successfully");
            setIsCreateDialogOpen(false);
          }}
        />
      </div>
    </CanWorkspaceAdmin>
  );
};

function NoPermission({ label }: { label: string }) {
  return (
    <div className='flex h-full items-center justify-center p-8'>
      <p className='text-muted-foreground text-sm'>
        You need workspace admin access to manage {label}.
      </p>
    </div>
  );
}

export default SecretsPage;

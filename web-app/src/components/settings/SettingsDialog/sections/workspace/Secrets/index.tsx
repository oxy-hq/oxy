import { KeyRound, Plus } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { toast } from "sonner";
import { CanWorkspaceAdmin } from "@/components/auth/Can";
import { CreateSecretDialog } from "@/components/settings/secrets/CreateSecretDialog";
import { UnifiedSecretsTable } from "@/components/settings/secrets/UnifiedSecretsTable";
import { Button } from "@/components/ui/shadcn/button";
import SectionHeader from "../../../components/SectionHeader";

const Secrets: React.FC = () => {
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);

  return (
    <CanWorkspaceAdmin
      fallback={
        <div className='flex items-center justify-center py-12'>
          <p className='text-muted-foreground text-sm'>
            You need workspace admin access to manage secrets.
          </p>
        </div>
      }
    >
      <div className='flex flex-col gap-5'>
        <SectionHeader
          icon={KeyRound}
          title='Secrets'
          actions={
            <Button size='sm' variant='outline' onClick={() => setIsCreateDialogOpen(true)}>
              <Plus />
              Create
            </Button>
          }
        />

        <UnifiedSecretsTable />

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

export default Secrets;

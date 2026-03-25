import type React from "react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import PageWrapper from "../components/PageWrapper";
import { CreateSecretDialog } from "./CreateSecretDialog";
import { UnifiedSecretsTable } from "./UnifiedSecretsTable";

const SecretManagement: React.FC = () => {
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);

  return (
    <PageWrapper
      title='Secrets'
      actions={
        <Button
          size='sm'
          onClick={() => setIsCreateDialogOpen(true)}
          className='flex items-center gap-2'
        >
          Create
        </Button>
      }
    >
      <UnifiedSecretsTable />

      <CreateSecretDialog
        open={isCreateDialogOpen}
        onOpenChange={setIsCreateDialogOpen}
        onSecretCreated={() => {
          toast.success("Secret created successfully");
          setIsCreateDialogOpen(false);
        }}
      />
    </PageWrapper>
  );
};

export default SecretManagement;

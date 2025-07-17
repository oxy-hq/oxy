import React, { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { toast } from "sonner";
import PageWrapper from "../components/PageWrapper";
import { SecretTable } from "./SecretTable";
import { CreateSecretDialog } from "./CreateSecretDialog";

const SecretManagement: React.FC = () => {
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);

  const handleSecretCreated = () => {
    toast.success("Secret created successfully");
    setIsCreateDialogOpen(false);
  };

  return (
    <PageWrapper
      title="Secrets"
      actions={
        <Button
          size="sm"
          onClick={() => setIsCreateDialogOpen(true)}
          className="flex items-center gap-2"
        >
          Create
        </Button>
      }
    >
      <div>
        <SecretTable />
        <CreateSecretDialog
          open={isCreateDialogOpen}
          onOpenChange={setIsCreateDialogOpen}
          onSecretCreated={handleSecretCreated}
        />
      </div>
    </PageWrapper>
  );
};

export default SecretManagement;

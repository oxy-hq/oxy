import { KeyRound, Plus } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { toast } from "sonner";
import { CreateSecretDialog } from "@/components/settings/secrets/CreateSecretDialog";
import { SecretTable } from "@/components/settings/secrets/SecretTable";
import { Button } from "@/components/ui/shadcn/button";
import useEnvSecrets from "@/hooks/api/secrets/useEnvSecrets";
import PageHeader from "@/pages/ide/components/PageHeader";

const SecretsPage: React.FC = () => {
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const { data: envSecrets } = useEnvSecrets();

  const handleSecretCreated = () => {
    toast.success("Secret created successfully");
    setIsCreateDialogOpen(false);
  };

  return (
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

      <div className='customScrollbar scrollbar-gutter-auto min-h-0 flex-1 space-y-6 overflow-auto p-4'>
        <SecretTable />

        {envSecrets && envSecrets.length > 0 && (
          <div>
            <div className='mb-3 flex items-center gap-2'>
              <span className='font-medium text-[11px] text-muted-foreground/60 uppercase tracking-widest'>
                Environment
              </span>
              <div className='h-px flex-1 bg-border/50' />
            </div>

            <div className='space-y-0.5'>
              {envSecrets.map((s) => (
                <div
                  key={s.env_var}
                  className='flex items-center justify-between rounded px-2 py-1.5 hover:bg-muted/40'
                >
                  <div className='flex min-w-0 items-baseline gap-1.5'>
                    <span className='font-mono text-xs'>{s.env_var}</span>
                    <span className='text-[10px] text-muted-foreground/50'>· {s.config_field}</span>
                  </div>
                  <span className='ml-3 shrink-0 font-mono text-[11px] text-muted-foreground/70 tabular-nums'>
                    {s.masked_value ?? <span className='text-muted-foreground/30'>unset</span>}
                  </span>
                </div>
              ))}
            </div>

            <p className='mt-3 text-[11px] text-muted-foreground/50 leading-relaxed'>
              To override an env value, create a secret with the same variable name.
            </p>
          </div>
        )}
      </div>

      <CreateSecretDialog
        open={isCreateDialogOpen}
        onOpenChange={setIsCreateDialogOpen}
        onSecretCreated={handleSecretCreated}
      />
    </div>
  );
};

export default SecretsPage;

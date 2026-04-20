import { useQueryClient } from "@tanstack/react-query";
import { Building2, Loader2, User } from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import {
  useConnectGitHubAccount,
  useCreateInstallationNamespace,
  useGitHubAccount,
  useUserInstallations
} from "@/hooks/api/github";
import queryKeys from "@/hooks/api/queryKey";
import { GitHubApiService } from "@/services/api";
import { openSecureWindow } from "@/utils/githubAppInstall";
import { GitHubCallbackCancelled, waitForGitHubCallback } from "@/utils/githubCallbackMessage";
import ConnectGitHubAccountStep from "./ConnectGitHubAccountStep";

interface Props {
  orgId: string;
  open: boolean;
  onClose: () => void;
  onConnected: (namespaceId: string) => void;
}

export default function AddViaInstallationDialog({ orgId, open, onClose, onConnected }: Props) {
  const { data: account } = useGitHubAccount();
  const isConnected = account?.connected === true;

  const {
    data: installations = [],
    isLoading,
    isError
  } = useUserInstallations({
    enabled: isConnected && open
  });

  const { mutateAsync: createNamespace, isPending: isCreating } = useCreateInstallationNamespace();
  const { mutate: reconnect, isPending: isReconnecting } = useConnectGitHubAccount();
  const queryClient = useQueryClient();

  const handlePick = async (installationId: number) => {
    const ns = await createNamespace({ orgId, installationId });
    onConnected(ns.id);
  };

  const handleInstallAnother = async () => {
    try {
      const url = await GitHubApiService.getNewInstallationUrl(orgId, window.location.origin);
      const popup = openSecureWindow(url);
      const result = await waitForGitHubCallback(popup, "install");
      queryClient.invalidateQueries({ queryKey: queryKeys.github.namespaces(orgId) });
      queryClient.invalidateQueries({ queryKey: queryKeys.github.userInstallations });
      onConnected(result.namespace_id);
    } catch (err: unknown) {
      if (err instanceof GitHubCallbackCancelled) return;
      const message = err instanceof Error ? err.message : "Failed to install GitHub App";
      toast.error(message);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle>Connect via GitHub App</DialogTitle>
          <DialogDescription>Select a GitHub account or organization to connect.</DialogDescription>
        </DialogHeader>

        {!isConnected && <ConnectGitHubAccountStep orgId={orgId} />}

        {isConnected && isLoading && (
          <div className='flex items-center justify-center gap-2 py-4 text-muted-foreground text-sm'>
            <Loader2 className='h-4 w-4 animate-spin' />
            Loading accounts…
          </div>
        )}

        {isConnected && isError && (
          <div className='space-y-3'>
            <div className='rounded-lg border border-border/60 bg-muted/30 px-3 py-3'>
              <p className='font-medium text-sm'>Could not load GitHub accounts</p>
              <p className='mt-0.5 text-muted-foreground text-xs leading-relaxed'>
                There was an error fetching your GitHub installations. Try reconnecting.
              </p>
            </div>
            <Button
              className='w-full gap-2'
              onClick={() => reconnect({ orgId })}
              disabled={isReconnecting}
            >
              {isReconnecting && <Loader2 className='h-4 w-4 animate-spin' />}
              Reconnect GitHub
            </Button>
          </div>
        )}

        {isConnected && !isLoading && !isError && installations.length === 0 && (
          <div className='space-y-3'>
            <div className='rounded-lg border border-border/60 bg-muted/30 px-3 py-3'>
              <p className='font-medium text-sm'>GitHub App not installed</p>
              <p className='mt-0.5 text-muted-foreground text-xs leading-relaxed'>
                The GitHub App is not installed on any account you can access. Install it on your
                personal account or organization, then come back.
              </p>
            </div>
            <Button className='w-full gap-2' onClick={handleInstallAnother}>
              Install GitHub App
            </Button>
          </div>
        )}

        {isConnected && !isLoading && !isError && installations.length > 0 && (
          <div className='space-y-2'>
            <p className='text-muted-foreground text-xs'>Choose an account:</p>
            <div className='flex flex-col gap-1.5'>
              {installations.map((inst) => (
                <button
                  key={inst.id}
                  type='button'
                  onClick={() => handlePick(inst.id)}
                  disabled={isCreating}
                  className='flex items-center gap-3 rounded-lg border border-border px-3 py-2.5 text-left transition-colors hover:border-primary hover:bg-primary/5 disabled:opacity-50'
                >
                  {inst.account_type === "Organization" ? (
                    <Building2 className='h-4 w-4 shrink-0 text-muted-foreground' />
                  ) : (
                    <User className='h-4 w-4 shrink-0 text-muted-foreground' />
                  )}
                  <span className='flex-1 font-medium text-sm'>{inst.account_login}</span>
                  <Badge variant='outline' className='shrink-0 text-xs'>
                    {inst.account_type}
                  </Badge>
                  {isCreating && (
                    <Loader2 className='h-3.5 w-3.5 animate-spin text-muted-foreground' />
                  )}
                </button>
              ))}
            </div>
            <Button variant='outline' className='w-full gap-2' onClick={handleInstallAnother}>
              + Install on another org
            </Button>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

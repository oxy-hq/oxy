import { Database, GitMerge, KeyRound } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/shadcn/dialog";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";
import type { WorkspaceCreationType } from "../types";

interface Props {
  open: boolean;
  workspaceId: string | null;
  workspaceType: WorkspaceCreationType | null;
  onClose: () => void;
}

export function WorkspaceReminderDialog({ open, workspaceId, workspaceType, onClose }: Props) {
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const routes = workspaceId ? ROUTES.ORG(orgSlug).WORKSPACE(workspaceId) : null;
  const isDemo = workspaceType === "demo";
  const isGithub = workspaceType === "github";

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <div className='mb-1 flex h-9 w-9 items-center justify-center rounded-lg border border-primary/20 bg-primary/5'>
            <GitMerge className='h-4 w-4 text-primary' />
          </div>
          <DialogTitle className='font-semibold text-base'>
            {isDemo
              ? "Demo workspace ready"
              : isGithub
                ? "Repository imported"
                : "Workspace created"}
          </DialogTitle>
        </DialogHeader>

        <p className='text-muted-foreground text-sm leading-relaxed'>
          {isDemo
            ? "Your demo workspace is set up with sample data. Here's what's included and what you can configure:"
            : isGithub
              ? "The repository is being cloned in the background. While you wait, set up a few things:"
              : "Your workspace is ready. Set up a few things to get the most out of Oxy:"}
        </p>

        <div className='flex flex-col gap-2'>
          <div className='flex items-start gap-3 rounded-lg border border-border bg-muted/30 px-3.5 py-3'>
            <div className='mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md border border-border bg-background'>
              <KeyRound className='h-3 w-3 text-muted-foreground' />
            </div>
            <div className='min-w-0 flex-1'>
              <p className='font-medium text-[13px] text-foreground'>LLM API key</p>
              <p className='mt-0.5 text-muted-foreground text-xs leading-relaxed'>
                Add your LLM provider key in Settings → Secrets.
              </p>
            </div>
            {routes && (
              <a
                href={routes.IDE.SETTINGS.SECRETS}
                target='_blank'
                rel='noopener noreferrer'
                className='mt-0.5 shrink-0 text-primary text-xs hover:underline'
              >
                Set up ↗
              </a>
            )}
          </div>

          <div className='flex items-start gap-3 rounded-lg border border-border bg-muted/30 px-3.5 py-3'>
            <div className='mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md border border-border bg-background'>
              <Database className='h-3 w-3 text-muted-foreground' />
            </div>
            <div className='min-w-0 flex-1'>
              {isDemo ? (
                <>
                  <p className='font-medium text-[13px] text-foreground'>Sample data included</p>
                  <p className='mt-0.5 text-muted-foreground text-xs leading-relaxed'>
                    Pre-loaded DuckDB databases with retail and sales data. No setup required.
                  </p>
                </>
              ) : (
                <>
                  <p className='font-medium text-[13px] text-foreground'>Database connection</p>
                  <p className='mt-0.5 text-muted-foreground text-xs leading-relaxed'>
                    Add a database so agents can run SQL queries.
                  </p>
                </>
              )}
            </div>
            {!isDemo && routes && (
              <a
                href={routes.IDE.SETTINGS.DATABASES}
                target='_blank'
                rel='noopener noreferrer'
                className='mt-0.5 shrink-0 text-primary text-xs hover:underline'
              >
                Set up ↗
              </a>
            )}
          </div>
        </div>

        {isGithub && (
          <p className='text-muted-foreground/60 text-xs'>
            You can open the workspace once cloning completes. The card will update automatically.
          </p>
        )}

        <div className='flex justify-end gap-2 pt-1'>
          <Button onClick={onClose} size='sm' className='h-8 px-4 text-xs'>
            Got it
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

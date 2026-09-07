import { Loader2, MoreHorizontal } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from "@/components/ui/shadcn/alert-dialog";
import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";
import { useSetWorkerStanding } from "@/hooks/api/organizations";
import type { AppAccessSummary } from "@/types/appAccess";
import type { FrontlineWorker } from "@/types/frontline";
import { ResetPinDialog } from "./ResetPinDialog";
import { WorkerAppsDialog } from "./WorkerAppsDialog";

/**
 * Everything an admin does to one worker after enrolment. Suspend asks first
 * — it locks a person out of their shift — while Reinstate is one click, as
 * undoing a lockout should be.
 */
export function WorkerRowActions({
  worker,
  orgId,
  apps
}: {
  worker: FrontlineWorker;
  orgId: string;
  apps: AppAccessSummary[];
}) {
  const [appsOpen, setAppsOpen] = useState(false);
  const [pinOpen, setPinOpen] = useState(false);
  const [suspendOpen, setSuspendOpen] = useState(false);
  const setStanding = useSetWorkerStanding();
  const suspended = worker.status === "suspended";

  const handleStanding = (active: boolean) => {
    setStanding.mutate(
      { orgId, userId: worker.user_id, active },
      {
        onSuccess: () =>
          toast.success(active ? `Reinstated ${worker.name}` : `Suspended ${worker.name}`),
        onError: () =>
          toast.error(active ? "Couldn't reinstate the worker" : "Couldn't suspend the worker")
      }
    );
  };

  return (
    <>
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger asChild>
          <Button
            variant='ghost'
            size='icon'
            className='h-8 w-8 data-[state=open]:bg-muted'
            disabled={setStanding.isPending}
          >
            {setStanding.isPending ? (
              <Loader2 className='h-4 w-4 animate-spin' />
            ) : (
              <MoreHorizontal className='h-4 w-4' />
            )}
            <span className='sr-only'>Open menu</span>
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align='end' className='w-44'>
          <DropdownMenuItem onClick={() => setAppsOpen(true)}>Apps…</DropdownMenuItem>
          <DropdownMenuItem onClick={() => setPinOpen(true)}>Reset PIN…</DropdownMenuItem>
          <DropdownMenuSeparator />
          {suspended ? (
            <DropdownMenuItem onClick={() => handleStanding(true)}>Reinstate</DropdownMenuItem>
          ) : (
            <DropdownMenuItem
              className='text-destructive focus:text-destructive'
              onClick={() => setSuspendOpen(true)}
            >
              Suspend
            </DropdownMenuItem>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      <WorkerAppsDialog
        open={appsOpen}
        onOpenChange={setAppsOpen}
        orgId={orgId}
        worker={worker}
        apps={apps}
      />
      <ResetPinDialog open={pinOpen} onOpenChange={setPinOpen} orgId={orgId} worker={worker} />

      <AlertDialog open={suspendOpen} onOpenChange={setSuspendOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Suspend {worker.name}?</AlertDialogTitle>
            <AlertDialogDescription>
              They can't sign in on any kiosk until you reinstate them. Their PIN and apps are kept.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              className='bg-destructive text-destructive-foreground hover:bg-destructive/90'
              onClick={() => handleStanding(false)}
              data-testid='settings-crew-suspend-confirm'
            >
              Suspend
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

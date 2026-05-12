import { VisuallyHidden } from "@radix-ui/react-visually-hidden";
import { X } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { CanOrgAdmin } from "@/components/auth/Can";
import ErrorAlert from "@/components/ui/ErrorAlert";
import { Button } from "@/components/ui/shadcn/button";
import { Dialog, DialogClose, DialogContent, DialogTitle } from "@/components/ui/shadcn/dialog";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useAllWorkspaces, useDeleteWorkspace } from "@/hooks/api/workspaces/useWorkspaces";
import { clearLastWorkspaceId } from "@/libs/utils/lastWorkspace";
import ROUTES from "@/libs/utils/routes";
import type { WorkspaceSummary } from "@/services/api/workspaces";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import useManageWorkspacesDialog from "@/stores/useManageWorkspacesDialog";
import { CreateWorkspaceDialog } from "../CreateWorkspaceDialog";
import { NewWorkspaceCard } from "./components/NewWorkspaceCard";
import { WorkspaceCard } from "./components/WorkspaceCard";

export function ManageWorkspacesDialog() {
  const { isOpen, close: closeDialog } = useManageWorkspacesDialog();
  const orgId = useCurrentOrg((s) => s.org?.id);
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const navigate = useNavigate();
  const { workspace: currentWorkspace } = useCurrentWorkspace();
  const { data: workspaces = [], isPending, isError, refetch } = useAllWorkspaces(orgId);
  const { mutate: deleteWorkspace, isPending: isDeleting } = useDeleteWorkspace();
  const [createOpen, setCreateOpen] = useState(false);

  const handleSwitch = (workspace: WorkspaceSummary) => {
    if (!workspace.org_id) return;
    if (workspace.id !== currentWorkspace?.id) {
      navigate(ROUTES.ORG(orgSlug).WORKSPACE(workspace.id).ROOT);
    }
    closeDialog();
  };

  const handleDelete = (workspace: WorkspaceSummary) => {
    if (!workspace.org_id) return;
    const isCurrent = workspace.id === currentWorkspace?.id;

    // The mutation's onMutate synchronously removes this workspace from the
    // cached list, so the navigate below mounts OrgDispatcher with an
    // already-trimmed list and it picks a different workspace instead of
    // looping back to the just-deleted one.
    deleteWorkspace(
      { orgId: workspace.org_id, id: workspace.id, deleteFiles: true },
      {
        onError: () => {
          toast.error("Failed to delete workspace. Please try again.");
        }
      }
    );

    if (isCurrent) {
      clearLastWorkspaceId(workspace.org_id);
      closeDialog();
      navigate(ROUTES.ORG(orgSlug).ROOT);
    }
  };

  return (
    <>
      <Dialog open={isOpen} onOpenChange={(o) => !o && closeDialog()}>
        <DialogContent
          className='top-0 left-0 flex h-[100svh] w-screen max-w-none translate-x-0 translate-y-0 flex-col gap-0 overflow-hidden rounded-none p-0 sm:top-1/2 sm:left-1/2 sm:h-[min(620px,100vh)] sm:max-w-4xl sm:-translate-x-1/2 sm:-translate-y-1/2 sm:gap-8 sm:rounded-lg sm:p-6'
          showCloseButton={false}
        >
          <VisuallyHidden>
            <DialogTitle>Manage workspaces</DialogTitle>
          </VisuallyHidden>

          {/* Mobile header — full-page chrome with a real close button.
              Hidden on sm+ where the dialog falls back to a centered modal. */}
          <header className='flex h-14 shrink-0 items-center gap-1 border-border border-b bg-background px-2 sm:hidden'>
            <div className='h-10 w-10' aria-hidden='true' />
            <h1 className='min-w-0 flex-1 truncate text-center font-semibold text-base text-foreground'>
              Manage workspaces
            </h1>
            <DialogClose asChild>
              <Button
                variant='ghost'
                size='icon'
                aria-label='Close'
                className='h-10 w-10 shrink-0 text-muted-foreground hover:text-foreground'
              >
                <X className='h-5 w-5' />
              </Button>
            </DialogClose>
          </header>

          {/* Desktop title — sm+ only. */}
          <h2 className='hidden font-semibold text-foreground text-lg sm:block'>
            Manage workspaces
          </h2>

          {isPending ? (
            <div className='flex min-h-60 w-full flex-1 items-center justify-center'>
              <Spinner className='text-muted-foreground' />
            </div>
          ) : isError ? (
            <div className='flex min-h-60 w-full flex-1 items-center justify-center p-4'>
              <ErrorAlert message='Failed to load workspaces.' />
            </div>
          ) : (
            <div className='grid flex-1 grid-cols-1 content-start gap-4 overflow-y-auto p-4 sm:grid-cols-2 sm:p-0 lg:grid-cols-3'>
              {workspaces.map((workspace, index) => (
                <WorkspaceCard
                  key={workspace.id}
                  workspace={workspace}
                  index={index}
                  isActive={workspace.id === currentWorkspace?.id}
                  onSwitch={() => handleSwitch(workspace)}
                  onDelete={() => handleDelete(workspace)}
                  isDeleting={isDeleting}
                />
              ))}
              <CanOrgAdmin>
                <NewWorkspaceCard index={workspaces.length} onClick={() => setCreateOpen(true)} />
              </CanOrgAdmin>
            </div>
          )}
        </DialogContent>
      </Dialog>

      <CreateWorkspaceDialog
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        onCreated={() => refetch()}
      />
    </>
  );
}

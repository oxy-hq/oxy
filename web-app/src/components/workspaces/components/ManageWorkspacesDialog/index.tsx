import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { CanOrgAdmin } from "@/components/auth/Can";
import ErrorAlert from "@/components/ui/ErrorAlert";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/shadcn/dialog";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useAllWorkspaces, useDeleteWorkspace } from "@/hooks/api/workspaces/useWorkspaces";
import ROUTES from "@/libs/utils/routes";
import type { WorkspaceSummary } from "@/services/api/workspaces";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import { CreateWorkspaceDialog } from "../CreateWorkspaceDialog";
import { NewWorkspaceCard } from "./components/NewWorkspaceCard";
import { WorkspaceCard } from "./components/WorkspaceCard";

type Props = {
  open: boolean;
  onClose: () => void;
};

export function ManageWorkspacesDialog({ open, onClose }: Props) {
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
    onClose();
  };

  const handleDelete = (workspace: WorkspaceSummary) => {
    if (!workspace.org_id) return;
    const isCurrent = workspace.id === currentWorkspace?.id;
    deleteWorkspace(
      { orgId: workspace.org_id, id: workspace.id, deleteFiles: true },
      {
        onSuccess: () => {
          if (isCurrent) {
            onClose();
            // OrgDispatcher at /:orgSlug picks another workspace or routes to
            // onboarding if none remain. Without this, we leave the user on a
            // URL pointing to a now-deleted workspace.
            navigate(ROUTES.ORG(orgSlug).ROOT);
          }
        },
        onError: () => {
          toast.error("Failed to delete workspace. Please try again.");
        }
      }
    );
  };

  return (
    <>
      <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
        <DialogContent className='flex h-[min(620px,100vh)] flex-col gap-8 sm:max-w-4xl'>
          <DialogHeader>
            <DialogTitle>Manage workspaces</DialogTitle>
          </DialogHeader>

          {isPending ? (
            <div className='flex min-h-60 w-full items-center justify-center'>
              <Spinner className='text-muted-foreground' />
            </div>
          ) : isError ? (
            <div className='flex min-h-60 w-full items-center justify-center'>
              <ErrorAlert message='Failed to load workspaces.' />
            </div>
          ) : (
            <div className='grid flex-1 grid-cols-1 content-start gap-4 overflow-y-auto sm:grid-cols-2 lg:grid-cols-3'>
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

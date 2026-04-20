import { Loader2, Plus } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { useAllWorkspaces, useDeleteWorkspace } from "@/hooks/api/workspaces/useWorkspaces";
import ROUTES from "@/libs/utils/routes";
import type { WorkspaceSummary } from "@/services/api/workspaces";
import useCurrentOrg from "@/stores/useCurrentOrg";
import { CreateWorkspaceDialog } from "./components/CreateWorkspaceDialog";
import { NewWorkspaceCard } from "./components/NewWorkspaceCard";
import { WorkspaceCard } from "./components/WorkspaceCard";
import { WorkspaceReminderDialog } from "./components/WorkspaceReminderDialog";
import type { WorkspaceCreationType } from "./types";

export default function WorkspacesPage() {
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const navigate = useNavigate();
  const { data: workspaces = [], isPending, isError, refetch } = useAllWorkspaces();
  const { mutate: deleteWorkspace, isPending: isDeleting } = useDeleteWorkspace();
  const [createOpen, setCreateOpen] = useState(false);
  const [reminderWorkspaceId, setReminderWorkspaceId] = useState<string | null>(null);
  const [reminderType, setReminderType] = useState<WorkspaceCreationType | null>(null);

  const handleSwitch = (workspace: WorkspaceSummary) => {
    if (!workspace.org_id) return;
    navigate(ROUTES.ORG(orgSlug).WORKSPACE(workspace.id).ROOT);
  };

  const handleDelete = (workspace: WorkspaceSummary) => {
    if (!workspace.org_id) return;
    deleteWorkspace({ orgId: workspace.org_id, id: workspace.id, deleteFiles: true });
  };

  const handleCreated = (workspaceId: string, type: WorkspaceCreationType) => {
    setCreateOpen(false);
    refetch();
    setReminderWorkspaceId(workspaceId);
    setReminderType(type);
  };

  const handleReminderClose = () => {
    const id = reminderWorkspaceId;
    const type = reminderType;
    setReminderWorkspaceId(null);
    setReminderType(null);
    // GitHub workspaces are still cloning — stay on the list page.
    // For demo/new workspaces the workspace is immediately usable, navigate to the IDE.
    if (type !== "github" && id) {
      navigate(ROUTES.ORG(orgSlug).WORKSPACE(id).IDE.ROOT);
    }
  };

  if (isPending) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <Loader2 className='h-4 w-4 animate-spin text-muted-foreground' />
      </div>
    );
  }

  if (isError) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <p className='text-destructive text-sm'>Failed to load workspaces.</p>
      </div>
    );
  }

  return (
    <div className='mx-auto w-full max-w-4xl px-6 py-12'>
      <div className='mb-8 flex items-end justify-between'>
        <h1 className='font-semibold text-2xl tracking-tight'>Workspaces</h1>
        <Button onClick={() => setCreateOpen(true)} size='sm' className='h-8 gap-1.5 px-3 text-xs'>
          <Plus className='h-3.5 w-3.5' />
          New workspace
        </Button>
      </div>

      <ul className='grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3'>
        {workspaces.map((workspace, index) => (
          <WorkspaceCard
            key={workspace.id}
            workspace={workspace}
            index={index}
            onSwitch={() => handleSwitch(workspace)}
            onDelete={() => handleDelete(workspace)}
            isDeleting={isDeleting}
          />
        ))}
        <NewWorkspaceCard index={workspaces.length} onClick={() => setCreateOpen(true)} />
      </ul>

      <CreateWorkspaceDialog
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        onCreated={handleCreated}
      />

      <WorkspaceReminderDialog
        open={reminderWorkspaceId !== null}
        workspaceId={reminderWorkspaceId}
        workspaceType={reminderType}
        onClose={handleReminderClose}
      />
    </div>
  );
}

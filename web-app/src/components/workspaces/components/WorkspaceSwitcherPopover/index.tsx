import { Settings } from "lucide-react";
import { type ReactNode, useState } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/shadcn/popover";
import { Separator } from "@/components/ui/shadcn/separator";
import { ManageWorkspacesDialog } from "@/components/workspaces/components/ManageWorkspacesDialog";
import { useAllWorkspaces } from "@/hooks/api/workspaces/useWorkspaces";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import { WorkspaceRow } from "./WorkspaceRow";

type Props = {
  children: ReactNode;
};

export function WorkspaceSwitcherPopover({ children }: Props) {
  const [open, setOpen] = useState(false);
  const [manageOpen, setManageOpen] = useState(false);
  const { workspace: currentWorkspace } = useCurrentWorkspace();
  const orgId = useCurrentOrg((s) => s.org?.id);
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const { data: workspaces = [] } = useAllWorkspaces(orgId);
  const navigate = useNavigate();

  const handleSelect = (workspaceId: string) => {
    if (workspaceId === currentWorkspace?.id) {
      setOpen(false);
      return;
    }
    const target = workspaces.find((w) => w.id === workspaceId);
    if (!target?.org_id) {
      // List is cached per-org, so a missing org_id means the entry is stale
      // (probably mid-switch). Tell the user instead of silently swallowing
      // the click — the next list refresh will resolve it.
      toast.error("Workspace is unavailable. Try again in a moment.");
      return;
    }
    navigate(ROUTES.ORG(orgSlug).WORKSPACE(workspaceId).ROOT);
    setOpen(false);
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>{children}</PopoverTrigger>

      <PopoverContent side='bottom' align='start' sideOffset={4} className='w-56 p-1.5'>
        {workspaces.length > 0 && (
          <div className='mb-1'>
            {workspaces.map((w) => (
              <WorkspaceRow
                key={w.id}
                workspace={w}
                isActive={w.id === currentWorkspace?.id}
                onSelect={() => handleSelect(w.id)}
              />
            ))}
          </div>
        )}

        <Separator className='my-1' />

        <button
          type='button'
          onClick={() => {
            setOpen(false);
            setManageOpen(true);
          }}
          className='flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-muted-foreground text-sm transition-colors hover:bg-accent hover:text-foreground'
        >
          <Settings className='h-3.5 w-3.5' />
          <span>Manage workspaces</span>
        </button>
      </PopoverContent>

      <ManageWorkspacesDialog open={manageOpen} onClose={() => setManageOpen(false)} />
    </Popover>
  );
}

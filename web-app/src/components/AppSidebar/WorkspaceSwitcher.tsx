import { Check, ChevronDown, Plus } from "lucide-react";
import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/shadcn/popover";
import { Separator } from "@/components/ui/shadcn/separator";
import { useAuth } from "@/contexts/AuthContext";
import { useAllWorkspaces } from "@/hooks/api/workspaces/useWorkspaces";
import ROUTES from "@/libs/utils/routes";
import type { WorkspaceSummary } from "@/services/api/workspaces";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";

function WorkspaceRow({
  workspace,
  isActive,
  onSelect
}: {
  workspace: WorkspaceSummary;
  isActive: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type='button'
      onClick={onSelect}
      className='flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors hover:bg-accent'
    >
      <span
        className={`flex-1 truncate ${isActive ? "font-medium text-foreground" : "text-muted-foreground"}`}
      >
        {workspace.name}
      </span>
      {isActive ? <Check className='h-3 w-3 text-primary' /> : null}
    </button>
  );
}

export function WorkspaceSwitcher() {
  const [open, setOpen] = useState(false);
  const { isLocalMode } = useAuth();
  const { workspace: currentWorkspace } = useCurrentWorkspace();
  // `useAllWorkspaces` is already gated by `!!orgId`, so in local mode the
  // query never fires — no extra guard needed here.
  const { data: workspaces = [] } = useAllWorkspaces();
  const navigate = useNavigate();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";

  const displayName = currentWorkspace?.name ?? "Loading…";

  if (isLocalMode) {
    return (
      <div className='flex w-full items-center px-2 py-1.5'>
        <span className='flex-1 truncate text-left font-semibold text-[13px] text-sidebar-foreground'>
          {displayName}
        </span>
      </div>
    );
  }

  const handleSelect = (workspaceId: string) => {
    if (workspaceId === currentWorkspace?.id) {
      setOpen(false);
      return;
    }
    const target = workspaces.find((w) => w.id === workspaceId);
    if (!target?.org_id) return;
    navigate(ROUTES.ORG(orgSlug).WORKSPACE(workspaceId).ROOT);
    setOpen(false);
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type='button'
          className='group flex w-full items-center gap-1.5 rounded-md border border-transparent px-2 py-1.5 transition-all hover:border-sidebar-border hover:bg-sidebar-accent focus-visible:outline-none focus-visible:ring-0'
          aria-label='Switch workspace'
          title='Switch workspace'
        >
          <span className='flex-1 truncate text-left font-semibold text-[13px] text-sidebar-foreground'>
            {displayName}
          </span>
          <ChevronDown className='h-3.5 w-3.5 shrink-0 text-sidebar-foreground/50 transition-transform group-hover:text-sidebar-foreground/80 group-data-[state=open]:rotate-180' />
        </button>
      </PopoverTrigger>

      <PopoverContent side='bottom' align='start' sideOffset={4} className='w-56 p-1.5'>
        {workspaces.length > 0 && (
          <div className='mb-1'>
            <p className='px-2 pb-1 font-medium text-[11px] text-muted-foreground uppercase tracking-wide'>
              Workspaces
            </p>
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

        <Link
          to={ROUTES.ORG(orgSlug).WORKSPACES}
          onClick={() => setOpen(false)}
          className='flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-muted-foreground text-sm transition-colors hover:bg-accent hover:text-foreground'
        >
          <Plus className='h-3.5 w-3.5' />
          <span>Manage workspaces</span>
        </Link>
      </PopoverContent>
    </Popover>
  );
}

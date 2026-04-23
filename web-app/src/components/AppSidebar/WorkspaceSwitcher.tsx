import { ChevronDown } from "lucide-react";
import { WorkspaceSwitcherPopover } from "@/components/workspaces/components/WorkspaceSwitcherPopover";
import { useAuth } from "@/contexts/AuthContext";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";

export function WorkspaceSwitcher() {
  const { isLocalMode } = useAuth();
  const { workspace: currentWorkspace } = useCurrentWorkspace();

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

  return (
    <WorkspaceSwitcherPopover>
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
    </WorkspaceSwitcherPopover>
  );
}

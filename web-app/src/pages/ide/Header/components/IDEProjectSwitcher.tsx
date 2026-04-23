import { ChevronDown, FolderOpen } from "lucide-react";
import { WorkspaceSwitcherPopover } from "@/components/workspaces/components/WorkspaceSwitcherPopover";
import { useAuth } from "@/contexts/AuthContext";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";

export function IDEProjectSwitcher() {
  const { isLocalMode } = useAuth();
  const { workspace: currentProject } = useCurrentWorkspace();
  const displayName = currentProject?.name ?? "…";

  if (isLocalMode) {
    return (
      <div className='flex h-7 max-w-44 items-center gap-1.5 px-2'>
        <FolderOpen className='h-3.5 w-3.5 shrink-0 text-muted-foreground/60' />
        <span className='min-w-0 flex-1 truncate text-left text-muted-foreground text-xs'>
          {displayName}
        </span>
      </div>
    );
  }

  return (
    <WorkspaceSwitcherPopover>
      <button
        type='button'
        className='group flex h-7 max-w-44 items-center gap-1.5 rounded border border-border/50 bg-transparent px-2 text-sm transition-colors hover:border-border hover:bg-accent/40'
        aria-label='Switch workspace'
      >
        <FolderOpen className='h-3.5 w-3.5 shrink-0 text-muted-foreground/60' />
        <span className='min-w-0 flex-1 truncate text-left text-muted-foreground text-xs'>
          {displayName}
        </span>
        <ChevronDown className='h-3 w-3 shrink-0 text-muted-foreground/40 transition-transform group-data-[state=open]:rotate-180' />
      </button>
    </WorkspaceSwitcherPopover>
  );
}

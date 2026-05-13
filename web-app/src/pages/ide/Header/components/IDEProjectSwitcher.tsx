import { ChevronDown, FolderOpen } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { WorkspaceSwitcherPopover } from "@/components/workspaces/components/WorkspaceSwitcherPopover";
import { useAuth } from "@/contexts/AuthContext";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";

export function IDEProjectSwitcher() {
  const { isLocalMode } = useAuth();
  const { workspace: currentProject } = useCurrentWorkspace();
  const displayName = currentProject?.name ?? "…";

  if (isLocalMode) {
    return (
      <div className='flex max-w-44 items-center px-2'>
        <span className='min-w-0 flex-1 truncate text-left text-sm'>{displayName}</span>
      </div>
    );
  }

  return (
    <WorkspaceSwitcherPopover>
      <Button size='sm' variant='outline' className='font-normal' aria-label='Switch workspace'>
        <FolderOpen />
        {displayName}
        <ChevronDown />
      </Button>
    </WorkspaceSwitcherPopover>
  );
}

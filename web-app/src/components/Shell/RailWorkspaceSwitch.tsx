import { ArrowLeftRight } from "lucide-react";
import { WorkspaceSwitcherPopover } from "@/components/workspaces/components/WorkspaceSwitcherPopover";
import { useAuth } from "@/contexts/AuthContext";

/** Bottom-cluster workspace switcher — cloud only (local mode has a
 *  single implicit workspace). The rail-top tile is pure branding;
 *  switching lives here, next to the user menu. */
export function RailWorkspaceSwitch() {
  const { isLocalMode } = useAuth();
  if (isLocalMode) return null;
  return (
    <WorkspaceSwitcherPopover>
      <button
        type='button'
        data-testid='rail-workspace-switch'
        aria-label='Switch workspace'
        title='Switch workspace'
        className='flex h-8 w-8 items-center justify-center rounded-md opacity-60 transition-opacity hover:bg-sidebar-accent hover:opacity-100'
      >
        <ArrowLeftRight className='h-4 w-4' />
      </button>
    </WorkspaceSwitcherPopover>
  );
}

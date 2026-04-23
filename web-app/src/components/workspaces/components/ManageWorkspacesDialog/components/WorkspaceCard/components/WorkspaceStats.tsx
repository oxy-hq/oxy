import { Bot, LayoutDashboard, Workflow } from "lucide-react";
import type { WorkspaceSummary } from "@/services/api/workspaces";

export function WorkspaceStats({ workspace }: { workspace: WorkspaceSummary }) {
  const hasAny =
    workspace.agent_count > 0 || workspace.workflow_count > 0 || workspace.app_count > 0;
  if (!hasAny) return null;

  return (
    <div className='flex items-center gap-3 pt-0.5'>
      {workspace.agent_count > 0 && (
        <span className='flex items-center gap-1 text-muted-foreground/50 text-xs'>
          <Bot className='size-3' />
          {workspace.agent_count}
        </span>
      )}
      {workspace.workflow_count > 0 && (
        <span className='flex items-center gap-1 text-muted-foreground/50 text-xs'>
          <Workflow className='size-3' />
          {workspace.workflow_count}
        </span>
      )}
      {workspace.app_count > 0 && (
        <span className='flex items-center gap-1 text-muted-foreground/50 text-xs'>
          <LayoutDashboard className='size-3' />
          {workspace.app_count}
        </span>
      )}
    </div>
  );
}

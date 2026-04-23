import { AlertTriangle, Check, FileWarning } from "lucide-react";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import type { WorkspaceSummary } from "@/services/api/workspaces";
import { WorkspaceTooltipContent } from "./WorkspaceTooltip";

export function WorkspaceRow({
  workspace,
  isActive,
  onSelect
}: {
  workspace: WorkspaceSummary;
  isActive: boolean;
  onSelect: () => void;
}) {
  const isCloning = workspace.status === "cloning";
  return (
    <Tooltip delayDuration={300}>
      <TooltipTrigger asChild>
        <button
          type='button'
          onClick={onSelect}
          disabled={isCloning}
          className='flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors hover:bg-accent disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:bg-transparent'
        >
          <span
            className={`flex-1 truncate ${isActive ? "font-medium text-foreground" : "text-muted-foreground"}`}
          >
            {workspace.name}
          </span>
          {isCloning ? (
            <Spinner className='size-3 shrink-0 text-muted-foreground' />
          ) : workspace.status === "failed" ? (
            <AlertTriangle className='h-3 w-3 shrink-0 text-destructive' />
          ) : workspace.status === "not_oxy_project" ? (
            <FileWarning className='h-3 w-3 shrink-0 text-warning' />
          ) : isActive ? (
            <Check className='h-3 w-3 text-primary' />
          ) : null}
        </button>
      </TooltipTrigger>
      <TooltipContent
        className='max-w-64 bg-card p-3'
        arrowClassName='bg-card fill-card'
        side='right'
        sideOffset={8}
      >
        <WorkspaceTooltipContent workspace={workspace} />
      </TooltipContent>
    </Tooltip>
  );
}

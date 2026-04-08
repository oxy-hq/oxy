import { CircleX } from "lucide-react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import { useWorkspaceStatus } from "@/hooks/api/workspaces/useWorkspaceStatus";

const WorkspaceStatus = () => {
  const { data, isPending, error } = useWorkspaceStatus();

  if (isPending) {
    return null;
  }

  if (error) {
    return (
      <div className='flex items-center gap-2 border border-border bg-sidebar p-2'>
        <CircleX className='h-4 w-4 text-destructive' />
        <span className='text-destructive text-sm'>Failed to load workspace status.</span>
      </div>
    );
  }

  if (data?.is_config_valid) {
    return null;
  }

  return (
    <div className='cursor-pointer border border-border bg-sidebar p-2'>
      {data?.error && (
        <Tooltip>
          <TooltipTrigger asChild>
            <div className='flex items-center gap-2'>
              <CircleX className='h-4 w-4 text-destructive' />
              <span className='text-destructive text-sm'>
                Unable to load workspace configuration.
              </span>
            </div>
          </TooltipTrigger>
          <TooltipContent
            side='bottom'
            className='max-w-md bg-card'
            arrowClassName='bg-card fill-card'
          >
            <div className='break-words text-destructive text-xs'>{data.error}</div>
          </TooltipContent>
        </Tooltip>
      )}
    </div>
  );
};

export default WorkspaceStatus;

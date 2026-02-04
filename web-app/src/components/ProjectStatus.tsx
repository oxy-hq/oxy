import { CircleX } from "lucide-react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import { useProjectStatus } from "@/hooks/api/projects/useProjectStatus";

const ProjectStatus = () => {
  const { data, isPending, error } = useProjectStatus();

  if (isPending) {
    return null;
  }

  if (error) {
    return (
      <div className='flex items-center gap-2 border border-border bg-sidebar p-2'>
        <CircleX className='h-4 w-4 text-destructive' />
        <span className='text-destructive text-sm'>Failed to load project status.</span>
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
                Unable to load project configuration.
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

export default ProjectStatus;

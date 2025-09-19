import { useProjectStatus } from "@/hooks/api/projects/useProjectStatus";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/shadcn/tooltip";
import { CircleX } from "lucide-react";

const ProjectStatus = () => {
  const { data, isPending, error } = useProjectStatus();

  if (isPending) {
    return null;
  }

  if (error) {
    return (
      <div className="flex items-center gap-2 p-2  bg-sidebar border border-border">
        <CircleX className="h-4 w-4 text-destructive" />
        <span className="text-sm text-destructive">
          Failed to load project status.
        </span>
      </div>
    );
  }

  if (data?.is_config_valid) {
    return null;
  }

  return (
    <div className="p-2 bg-sidebar border border-border cursor-pointer">
      {data?.error && (
        <Tooltip>
          <TooltipTrigger asChild>
            <div className="flex items-center gap-2">
              <CircleX className="h-4 w-4 text-destructive" />
              <span className="text-sm text-destructive">
                Unable to load project configuration.
              </span>
            </div>
          </TooltipTrigger>
          <TooltipContent
            side="bottom"
            className="max-w-md bg-card"
            arrowClassName="bg-card fill-card"
          >
            <div className="text-xs break-words text-destructive">
              {data.error}
            </div>
          </TooltipContent>
        </Tooltip>
      )}
    </div>
  );
};

export default ProjectStatus;

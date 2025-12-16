import { Button } from "@/components/ui/shadcn/button";
import { Loader2, Play } from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/shadcn/tooltip";

interface HeaderActionsProps {
  onExecuteQuery: () => void;
  loading: boolean;
  disabled?: boolean;
  disabledMessage?: string;
}

const HeaderActions = ({
  onExecuteQuery,
  loading,
  disabled,
  disabledMessage,
}: HeaderActionsProps) => {
  return (
    <div className="flex items-center gap-2 whitespace-nowrap overflow-x-auto">
      <Tooltip>
        <TooltipTrigger asChild>
          <span>
            <Button
              size="sm"
              className="hover:text-muted-foreground flex-shrink-0"
              variant="ghost"
              disabled={loading || disabled}
              onClick={onExecuteQuery}
              title="Run query"
            >
              {loading ? (
                <Loader2 className="w-4 h-4 animate-[spin_0.3s_linear_infinite]" />
              ) : (
                <Play className="w-4 h-4" />
              )}
            </Button>
          </span>
        </TooltipTrigger>
        <TooltipContent>
          {disabled
            ? disabledMessage || "Select dimensions or measures to run query"
            : "Run query"}
        </TooltipContent>
      </Tooltip>
    </div>
  );
};

export default HeaderActions;

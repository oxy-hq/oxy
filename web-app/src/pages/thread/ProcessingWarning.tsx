import { useMemo } from "react";
import { RefreshCcw } from "lucide-react";
import dayjs from "dayjs";
import relativeTime from "dayjs/plugin/relativeTime";

import { ThreadItem } from "@/types/chat";
import { cn } from "@/libs/shadcn/utils";
import { Button } from "@/components/ui/shadcn/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/shadcn/tooltip";

dayjs.extend(relativeTime);

interface Props {
  thread: ThreadItem;
  isLoading: boolean;
  className?: string;
  onRefresh: () => void;
}

const ProcessingWarning = ({
  thread,
  isLoading,
  className,
  onRefresh,
}: Props) => {
  const shouldShowProcessingWarning = useMemo(
    () => thread.is_processing && !isLoading,
    [isLoading, thread.is_processing],
  );

  if (!shouldShowProcessingWarning) return null;

  return (
    <div
      className={cn(
        "w-full px-3 bg-blue-900/20 border border-blue-600/30 rounded-lg mb-2",
        className,
      )}
    >
      <div className="flex items-center justify-between">
        <span className="text-blue-100 text-sm font-medium">
          Thread is still processing. The last message may not be complete yet.
        </span>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="ghost" onClick={onRefresh}>
              <RefreshCcw />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Refresh to get the latest updates</TooltipContent>
        </Tooltip>
      </div>
    </div>
  );
};

export default ProcessingWarning;

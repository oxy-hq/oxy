import dayjs from "dayjs";
import relativeTime from "dayjs/plugin/relativeTime";
import { RefreshCcw } from "lucide-react";
import { useMemo } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import useThread from "@/hooks/api/threads/useThread";
import { cn } from "@/libs/shadcn/utils";

dayjs.extend(relativeTime);

interface Props {
  threadId: string;
  isLoading: boolean;
  className?: string;
  onRefresh: () => void;
}

const ProcessingWarning = ({ threadId, isLoading, className, onRefresh }: Props) => {
  const { data: thread, isFetching } = useThread(threadId ?? "");

  const shouldShowProcessingWarning = useMemo(
    () => !isFetching && thread && thread.is_processing && !isLoading,
    [isLoading, thread, isFetching]
  );

  if (!shouldShowProcessingWarning) return null;

  return (
    <div
      className={cn(
        "mb-2 w-full rounded-lg border border-blue-600/30 bg-blue-900/20 px-3",
        className
      )}
    >
      <div className='flex items-center justify-between'>
        <span className='font-medium text-blue-100 text-sm'>
          Thread is still processing. The last message may not be complete yet.
        </span>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant='ghost' onClick={onRefresh}>
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

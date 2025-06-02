import EmptyThreads from "./Empty";
import ThreadsSkeleton from "./Skeleton";
import ErrorState from "./Error";
import { cn } from "@/libs/shadcn/utils";
import ThreadList from "./ThreadList";
import { ThreadsResponse } from "@/types/chat";
import { UseQueryResult } from "@tanstack/react-query";

interface Props {
  queryResult: UseQueryResult<ThreadsResponse, Error>;
  selectedThreads: Set<string>;
  isSelectionMode: boolean;
  onThreadSelect: (threadId: string, selected: boolean) => void;
}

const ThreadsSection = ({
  queryResult,
  onThreadSelect,
  selectedThreads,
  isSelectionMode,
}: Props) => {
  const {
    data: threadsResponse,
    isSuccess,
    isFetching,
    isPending,
    isError,
    error,
  } = queryResult;

  const threads = threadsResponse?.threads;

  return (
    <div className="flex-1 flex flex-col overflow-auto customScrollbar">
      <div className="max-w-page-content mx-auto w-full pt-4 px-2">
        {isError && <ErrorState error={error} />}

        {isPending && <ThreadsSkeleton />}

        {isSuccess && (
          <div
            className={cn(
              `${isFetching ? "opacity-60 pointer-events-none" : ""} transition-opacity duration-200`,
            )}
          >
            {threads && threads.length > 0 ? (
              <ThreadList
                threads={threads}
                selectedThreads={selectedThreads}
                onThreadSelect={onThreadSelect}
                isSelectionMode={isSelectionMode}
              />
            ) : (
              <EmptyThreads />
            )}
          </div>
        )}
      </div>
    </div>
  );
};

export default ThreadsSection;

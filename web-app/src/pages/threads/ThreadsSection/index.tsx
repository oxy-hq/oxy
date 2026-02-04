import type { UseQueryResult } from "@tanstack/react-query";
import { cn } from "@/libs/shadcn/utils";
import type { ThreadsResponse } from "@/types/chat";
import EmptyThreads from "./Empty";
import ErrorState from "./Error";
import ThreadsSkeleton from "./Skeleton";
import ThreadList from "./ThreadList";

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
  isSelectionMode
}: Props) => {
  const { data: threadsResponse, isSuccess, isFetching, isPending, isError, error } = queryResult;

  const threads = threadsResponse?.threads;

  return (
    <div className='customScrollbar flex flex-1 flex-col overflow-auto'>
      <div className='mx-auto w-full max-w-page-content px-2 pt-4'>
        {isError && <ErrorState error={error} />}

        {isPending && <ThreadsSkeleton />}

        {isSuccess && (
          <div
            className={cn(
              `${isFetching ? "pointer-events-none opacity-60" : ""} transition-opacity duration-200`
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

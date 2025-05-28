import useThreads from "@/hooks/api/useThreads";
import { MessageSquare, MessagesSquare } from "lucide-react";
import { Link, useSearchParams } from "react-router-dom";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { Button } from "@/components/ui/shadcn/button";
import PageHeader from "@/components/PageHeader";
import { ThreadsPagination, ThreadList } from "@/components/Threads";
import { useCallback, useRef } from "react";

const Threads = () => {
  const [searchParams, setSearchParams] = useSearchParams();
  const page = parseInt(searchParams.get("page") || "1");
  const limit = Math.max(
    10,
    Math.min(100, parseInt(searchParams.get("limit") || "10")),
  );
  const scrollElementRef = useRef<HTMLDivElement>(null);

  const {
    data: threadsResponse,
    isLoading,
    isFetching,
    isError,
    error,
  } = useThreads(page, limit);

  const handlePageChange = useCallback(
    (newPage: number) => {
      const newParams = new URLSearchParams(searchParams);
      newParams.set("page", newPage.toString());
      setSearchParams(newParams);
    },
    [searchParams, setSearchParams],
  );

  const handleLimitChange = useCallback(
    (newLimit: number) => {
      const clampedLimit = Math.max(10, Math.min(100, newLimit));
      const newParams = new URLSearchParams(searchParams);
      newParams.set("limit", clampedLimit.toString());
      newParams.set("page", "1"); // Reset to first page when changing limit
      setSearchParams(newParams);
    },
    [searchParams, setSearchParams],
  );

  const threads = threadsResponse?.threads;
  const pagination = threadsResponse?.pagination;

  return (
    <div className="flex flex-col h-full">
      <PageHeader className="flex-col border-b border-border w-full">
        <div className="px-6 flex gap-[10px] items-center pt-8">
          <MessagesSquare className="w-9 h-9 min-w-9 min-h-9" strokeWidth={1} />
          <h1 className="text-3xl font-semibold">Threads</h1>
        </div>
      </PageHeader>

      {!isLoading && !isError && pagination && (
        <div className="w-full px-6 py-4">
          <ThreadsPagination
            pagination={pagination}
            onPageChange={handlePageChange}
            onLimitChange={handleLimitChange}
            currentLimit={limit}
            isLoading={isFetching}
          />
        </div>
      )}

      <div
        ref={scrollElementRef}
        className="overflow-y-auto customScrollbar flex-1"
      >
        <div className="w-full flex flex-col gap-6 px-6">
          {/* Error state */}
          {isError && (
            <div className="flex flex-col gap-4 p-6 items-center justify-center">
              <div className="text-red-500 text-center">
                <p className="text-lg font-semibold">Error loading threads</p>
                <p className="text-sm text-muted-foreground">
                  {error?.message || "Something went wrong"}
                </p>
              </div>
              <Button
                variant="outline"
                onClick={() => window.location.reload()}
              >
                Try again
              </Button>
            </div>
          )}

          {/* Loading state */}
          {isLoading && <ThreadsSkeleton />}

          {/* Content with loading overlay for pagination transitions */}
          {!isLoading && !isError && (
            <div
              className={`${isFetching ? "opacity-60 pointer-events-none" : ""} transition-opacity duration-200`}
            >
              {threads && threads.length > 0 ? (
                <ThreadList threads={threads} />
              ) : (
                <EmptyThreads />
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

const ThreadsSkeleton = () => {
  return (
    <div className="flex flex-col gap-10">
      {Array.from({ length: 6 }).map((_, index) => (
        <div key={index} className="flex flex-col gap-4">
          <Skeleton className="h-6 max-w-[120px]" />
          <Skeleton className="h-7 max-w-[400px]" />
          <div className="space-y-2">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 max-w-[300px]" />
          </div>
        </div>
      ))}
    </div>
  );
};

const EmptyThreads = () => {
  return (
    <div className="flex flex-col gap-6 p-6 items-center justify-center">
      <div className="w-[48px] h-[48px] flex p-2 rounded-md border border-border shadow-sm items-center justify-center">
        <MessageSquare />
      </div>
      <div className="flex flex-col gap-2 items-center">
        <p className="text-xl font-semibold">No threads</p>
        <p className="text-sm text-muted-foreground">
          Start by asking an agent of your choice a question
        </p>
      </div>
      <Button variant="outline" asChild>
        <Link to="/">Start a new thread</Link>
      </Button>
    </div>
  );
};

export default Threads;

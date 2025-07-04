import useThreads from "@/hooks/api/threads/useThreads";
import { useSearchParams } from "react-router-dom";
import BulkActionToolbar from "@/pages/threads/BulkActionToolbar";
import { useState } from "react";
import Header from "./Header";
import ThreadsPagination from "./Pagination";
import ThreadsSection from "./ThreadsSection";

const Threads = () => {
  const [searchParams, setSearchParams] = useSearchParams();
  const page = parseInt(searchParams.get("page") || "1");
  const limit = Math.max(
    10,
    Math.min(100, parseInt(searchParams.get("limit") || "10")),
  );

  const [selectedThreads, setSelectedThreads] = useState<Set<string>>(
    new Set(),
  );

  const [isSelectionMode, setIsSelectionMode] = useState(false);

  const [isSelectAllPages, setIsSelectAllPages] = useState(false);

  const queryResult = useThreads(page, limit);

  const { data: threadsResponse, isLoading, isFetching, isError } = queryResult;

  const threads = threadsResponse?.threads;
  const pagination = threadsResponse?.pagination;

  const handlePageChange = (newPage: number) => {
    const newParams = new URLSearchParams(searchParams);
    newParams.set("page", newPage.toString());
    setSearchParams(newParams);
  };

  const handleLimitChange = (newLimit: number) => {
    const clampedLimit = Math.max(10, Math.min(100, newLimit));
    const newParams = new URLSearchParams(searchParams);
    newParams.set("limit", clampedLimit.toString());
    newParams.set("page", "1");
    setSearchParams(newParams);
  };

  const handleThreadSelect = (threadId: string, selected: boolean) => {
    if (!isSelectionMode) {
      setIsSelectionMode(true);
    }

    setSelectedThreads((prev) => {
      const newSet = new Set(prev);
      if (selected) {
        newSet.add(threadId);
      } else {
        newSet.delete(threadId);
        setIsSelectAllPages(false);
      }
      return newSet;
    });
  };

  const handleSelectAllOnPage = (checked: boolean) => {
    if (!threads) return;

    if (checked) {
      const threadIds = threads.map((thread) => thread.id);
      setSelectedThreads(new Set(threadIds));
    } else {
      setSelectedThreads(new Set());
      setIsSelectAllPages(false);
    }
  };

  const handleSelectAllPages = (checked: boolean) => {
    setIsSelectAllPages(checked);
    if (!checked) {
      handleSelectAllOnPage(true);
    }
  };

  const handleClearSelection = () => {
    setSelectedThreads(new Set());
    setIsSelectAllPages(false);
    setIsSelectionMode(false);
  };

  const handleSelectMode = () => {
    setIsSelectionMode(true);
  };

  const selectedCount = selectedThreads.size;
  const totalOnPage = threads?.length || 0;
  const totalAcrossAll = pagination?.total;

  const shouldShowPagination = !isLoading && !isError && pagination;

  return (
    <div className="flex flex-col h-full gap-4 pb-4">
      <Header
        onSelect={handleSelectMode}
        isSelectionMode={isSelectionMode}
        onCancel={handleClearSelection}
      />

      {selectedCount > 0 && (
        <BulkActionToolbar
          totalOnPage={totalOnPage}
          totalAcrossAll={totalAcrossAll}
          isSelectAllPages={isSelectAllPages}
          selectedThreads={selectedThreads}
          onSelectAll={handleSelectAllOnPage}
          onSelectAllPages={handleSelectAllPages}
          onClearSelection={handleClearSelection}
        />
      )}

      <ThreadsSection
        isSelectionMode={isSelectionMode}
        queryResult={queryResult}
        selectedThreads={selectedThreads}
        onThreadSelect={handleThreadSelect}
      />

      {shouldShowPagination && (
        <ThreadsPagination
          pagination={pagination}
          currentLimit={limit}
          isLoading={isFetching}
          onPageChange={handlePageChange}
          onLimitChange={handleLimitChange}
        />
      )}
    </div>
  );
};

export default Threads;

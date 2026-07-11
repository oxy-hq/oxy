import { useEffect, useMemo, useState } from "react";
import useTraces from "@/hooks/api/traces/useTraces";
import type { Trace } from "@/services/api/traces";
import type { TimeRange } from "./components/TimeRangeControl";
import { MAX_COMPARE } from "./constants";
import { type StatusFilter, statusFilterToApi, type TraceView } from "./types";

const PAGE_SIZE = 10;
const CHART_LIMIT = 500;
const LIVE_INTERVAL_MS = 5000;
const SEARCH_DEBOUNCE_MS = 300;

interface UseTracesControllerArgs {
  enabled: boolean;
}

/**
 * Owns all Traces-surface UI state (Theme 3): time range, debounced search,
 * status filter, live-tail polling, view mode, paging, and compare selection.
 * Feeds two `useTraces` queries (paged list + wider chart window) that share
 * every filter so the charts always reflect the visible set.
 */
export function useTracesController({ enabled }: UseTracesControllerArgs) {
  const [timeRange, setTimeRange] = useState<TimeRange>({ kind: "preset", value: "30d" });
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState<StatusFilter>("all");
  const [live, setLive] = useState(false);
  const [view, setView] = useState<TraceView>("card");
  const [currentPage, setCurrentPage] = useState(1);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);

  // Debounce the search box so keystrokes don't hammer the API.
  useEffect(() => {
    const timer = setTimeout(() => setSearch(searchInput.trim()), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [searchInput]);

  // Any filter change reshapes the result set → back to page 1, drop selection.
  // biome-ignore lint/correctness/useExhaustiveDependencies: these are reset keys, not values read in the body
  useEffect(() => {
    setCurrentPage(1);
    setSelectedIds([]);
  }, [search, status, timeRange]);

  const apiStatus = statusFilterToApi(status) ?? "all";
  const range =
    timeRange.kind === "custom"
      ? { duration: undefined, from: timeRange.from, to: timeRange.to }
      : { duration: timeRange.value, from: undefined, to: undefined };
  // Pause live-tail while a compare selection is in progress: an incoming trace
  // must not scroll a selected row off the page and strand the selection (the
  // bar would count a trace the Compare action can no longer resolve).
  const refetchInterval: number | false =
    live && selectedIds.length === 0 ? LIVE_INTERVAL_MS : false;
  const offset = (currentPage - 1) * PAGE_SIZE;

  const sharedFilters = {
    status: apiStatus,
    enabled,
    duration: range.duration,
    from: range.from,
    to: range.to,
    search,
    refetchInterval
  };

  const listQuery = useTraces({ ...sharedFilters, limit: PAGE_SIZE, offset });
  const chartQuery = useTraces({ ...sharedFilters, limit: CHART_LIMIT, offset: 0 });

  const traces = listQuery.data?.items;
  const total = listQuery.data?.total ?? 0;

  // Any refetch (window focus, a late in-flight poll) can drop a selected trace
  // off the current page. Prune selection to what's actually visible so
  // selectedIds, compareTraces, and the selection cap never disagree.
  useEffect(() => {
    if (!traces) return;
    setSelectedIds((prev) => {
      const visible = prev.filter((id) => traces.some((t) => t.traceId === id));
      return visible.length === prev.length ? prev : visible;
    });
  }, [traces]);

  const toggleSelect = (id: string) =>
    setSelectedIds((prev) => {
      if (prev.includes(id)) return prev.filter((x) => x !== id);
      if (prev.length >= MAX_COMPARE) return prev;
      return [...prev, id];
    });

  const compareTraces = useMemo<Trace[]>(
    () => traces?.filter((t) => selectedIds.includes(t.traceId)) ?? [],
    [traces, selectedIds]
  );

  const handlePageChange = (page: number) => {
    setCurrentPage(page);
    setSelectedIds([]);
  };

  const filtersActive = search.length > 0 || status !== "all" || timeRange.kind === "custom";

  return {
    timeRange,
    setTimeRange,
    searchInput,
    setSearchInput,
    status,
    setStatus,
    live,
    setLive,
    view,
    setView,
    traces,
    total,
    isLoading: listQuery.isLoading,
    chartTraces: chartQuery.data?.items,
    isChartLoading: chartQuery.isLoading,
    currentPage,
    pageSize: PAGE_SIZE,
    handlePageChange,
    selectedIds,
    toggleSelect,
    clearSelection: () => setSelectedIds([]),
    compareTraces,
    filtersActive
  };
}

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { AdminExplorerService, type ExplorerQueryParams } from "@/services/api/adminExplorer";
import queryKeys from "../queryKey";

export const useExplorerThreads = (
  params: ExplorerQueryParams,
  options: { enabled?: boolean } = {}
) =>
  useQuery({
    queryKey: queryKeys.adminExplorer.threads(params),
    queryFn: () => AdminExplorerService.threads(params),
    enabled: options.enabled ?? true,
    // Cross-tenant scan — don't hammer it on every keystroke re-render.
    staleTime: 15_000,
    // Keep showing the previous page's rows while the next page loads, so
    // pagination doesn't flash a loading state on every click.
    placeholderData: keepPreviousData
  });

export const useExplorerRuns = (params: ExplorerQueryParams, options: { enabled?: boolean } = {}) =>
  useQuery({
    queryKey: queryKeys.adminExplorer.runs(params),
    queryFn: () => AdminExplorerService.runs(params),
    enabled: options.enabled ?? true,
    staleTime: 15_000,
    placeholderData: keepPreviousData
  });

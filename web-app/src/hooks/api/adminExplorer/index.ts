import { useQuery } from "@tanstack/react-query";
import { AdminExplorerService } from "@/services/api/adminExplorer";
import queryKeys from "../queryKey";

export const useExplorerThreads = (search: string, options: { enabled?: boolean } = {}) =>
  useQuery({
    queryKey: queryKeys.adminExplorer.threads(search),
    queryFn: () => AdminExplorerService.threads(search),
    enabled: options.enabled ?? true,
    // Cross-tenant scan — don't hammer it on every keystroke re-render.
    staleTime: 15_000
  });

export const useExplorerRuns = (
  search: string,
  status: string,
  options: { enabled?: boolean } = {}
) =>
  useQuery({
    queryKey: queryKeys.adminExplorer.runs(search, status),
    queryFn: () => AdminExplorerService.runs(search, status),
    enabled: options.enabled ?? true,
    staleTime: 15_000
  });

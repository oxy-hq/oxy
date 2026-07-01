import { useQuery } from "@tanstack/react-query";
import { WorkspaceHealthService } from "@/services/api/workspaceHealth";
import queryKeys from "../queryKey";

/**
 * Fetches the cross-tenant workspace health rollup from
 * `GET /admin/workspace-health`. Results are sorted worst-first by the
 * backend. Stale time is short (30s) — this is an operator console
 * surface where freshness matters.
 */
export const useWorkspaceHealth = () =>
  useQuery({
    queryKey: queryKeys.workspaceHealth.list(),
    queryFn: () => WorkspaceHealthService.list(),
    staleTime: 30_000
  });

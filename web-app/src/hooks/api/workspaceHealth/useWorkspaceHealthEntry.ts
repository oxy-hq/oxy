import { useQuery } from "@tanstack/react-query";
import { type WorkspaceHealthEntry, WorkspaceHealthService } from "@/services/api/workspaceHealth";
import queryKeys from "../queryKey";

/**
 * Selects a single workspace's health entry from the cross-tenant rollup
 * (`GET /admin/workspace-health`). The backend has no per-workspace health
 * endpoint, so the per-workspace Health tab derives its data from the same
 * cached list the rollup page uses — `select` narrows it to one entry.
 *
 * Returns `null` (not `undefined`) when the workspace is absent from the
 * rollup so the caller can distinguish "loaded, no health row" from "loading".
 */
export const useWorkspaceHealthEntry = (workspaceId: string) =>
  useQuery({
    queryKey: queryKeys.workspaceHealth.list(),
    queryFn: () => WorkspaceHealthService.list(),
    staleTime: 30_000,
    select: (data): WorkspaceHealthEntry | null =>
      data.workspaces.find((ws) => ws.workspace_id === workspaceId) ?? null
  });

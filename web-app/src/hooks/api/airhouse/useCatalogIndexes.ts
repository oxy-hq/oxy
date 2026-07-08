import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { isAxiosError } from "axios";
import { toast } from "sonner";
import { AirhouseService, type CatalogIndexesStatus } from "@/services/api/airhouse";
import queryKeys from "../queryKey";

/**
 * Current state of the workspace tenant's DuckLake catalog hot-path indexes.
 * Polls every 3s while a `CREATE/DROP INDEX CONCURRENTLY` is in flight
 * (`state: "building"`) so the UI shows the `building → ready/absent` transition
 * live, then stops polling once it settles.
 */
export const useCatalogIndexes = (workspaceId: string) =>
  useQuery({
    queryKey: queryKeys.airhouse.catalogIndexes(workspaceId),
    queryFn: () => AirhouseService.getCatalogIndexes(workspaceId),
    enabled: !!workspaceId,
    refetchInterval: (query) => (query.state.data?.state === "building" ? 3000 : false)
  });

/**
 * Toggle the catalog hot-path indexes on/off. Airhouse applies the change
 * asynchronously (CONCURRENTLY), so on enable we optimistically mark the status
 * `"building"` — this starts the poll above immediately instead of waiting for
 * the next GET to observe airhouse registering the index; the poll then
 * converges to the real `ready` state. On disable we just refetch (the drop is
 * fast).
 */
export const useSetCatalogIndexes = (workspaceId: string) => {
  const qc = useQueryClient();
  const key = queryKeys.airhouse.catalogIndexes(workspaceId);
  return useMutation({
    mutationFn: (enabled: boolean) => AirhouseService.setCatalogIndexes(workspaceId, enabled),
    onSuccess: (_, enabled) => {
      if (enabled) {
        qc.setQueryData<CatalogIndexesStatus>(key, (prev) => ({
          state: "building",
          indexes: prev?.indexes ?? []
        }));
      } else {
        qc.invalidateQueries({ queryKey: key });
      }
      toast.success(enabled ? "Building catalog indexes…" : "Removing catalog indexes…");
    },
    onError: (err) => {
      const message = isAxiosError(err)
        ? (err.response?.data?.message ?? err.message)
        : err instanceof Error
          ? err.message
          : "Failed to update catalog indexes";
      toast.error(message);
    }
  });
};

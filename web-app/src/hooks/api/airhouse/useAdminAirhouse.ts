import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { isAxiosError } from "axios";
import queryKeys from "@/hooks/api/queryKey";
import { listAirhouseFleet, provisionAirhouseTenant } from "@/services/api/airhouseAdmin";

const fleetKey = queryKeys.airhouse.adminFleet;

/**
 * The server's own reason for a failure, or `fallback`.
 *
 * Exported because the provision dialog owns the toast now — one extraction,
 * so the message an operator reads cannot depend on which caller reported it.
 */
export function airhouseErrorMessage(err: unknown, fallback: string): string {
  // The provision handler returns its reason as a plain body — a name
  // collision or "Airhouse is not configured on this deployment" is exactly
  // what an operator needs to read, so surface it rather than a generic error.
  if (isAxiosError(err)) {
    const data = err.response?.data;
    if (typeof data === "string" && data) return data;
    return data?.message ?? err.message;
  }
  return err instanceof Error ? err.message : fallback;
}

/** Every workspace, with its Airhouse tenant when it has one. */
export function useAirhouseFleet() {
  return useQuery({
    queryKey: fleetKey(),
    queryFn: listAirhouseFleet,
    // Provisioning is slow and rare; nothing else mutates this behind our back.
    staleTime: 30_000
  });
}

export function useProvisionAirhouseTenant(workspaceId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => provisionAirhouseTenant(workspaceId),
    // Cache invalidation only. The caller owns the toast: this hook is driven
    // through `mutateAsync` in `ProvisionConfirmDialog`, which needs the
    // awaited result to close on success and already reports both outcomes —
    // toasting here too stacked two notifications on one click, and the pair
    // disagreed (this one names the derived tenant id, the dialog names the
    // workspace the operator actually clicked).
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: fleetKey() });
    }
  });
}

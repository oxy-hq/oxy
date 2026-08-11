import { useMutation, useQueryClient } from "@tanstack/react-query";
import { isAxiosError } from "axios";
import { toast } from "sonner";
import { AirwayConfigService, type UpsertAirwayConfigBody } from "@/services/api/airwayConfig";
import queryKeys from "../queryKey";

function errMessage(err: unknown, fallback: string): string {
  if (isAxiosError(err)) return err.response?.data?.message ?? err.message;
  if (err instanceof Error) return err.message;
  return fallback;
}

/**
 * Create or replace the global (`workspace_id IS NULL`) row for a source
 * kind (`PUT /admin/airway/config/{source_kind}`). `body` is a replace, not
 * a patch — callers must send both `contract_policy` and `environment`
 * every time; `null` on either clears it back to "inherit".
 */
export const useUpsertAirwayGlobalConfig = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ sourceKind, body }: { sourceKind: string; body: UpsertAirwayConfigBody }) =>
      AirwayConfigService.upsertGlobal(sourceKind, body),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.airwayConfig.config() });
      toast.success("Airway admission policy updated");
    },
    onError: (err) => toast.error(errMessage(err, "Failed to update airway admission policy"))
  });
};

/**
 * Delete the global row for a source kind
 * (`DELETE /admin/airway/config/{source_kind}`). A no-op if the row doesn't
 * exist. Leaves any per-workspace overrides in place — they just lose the
 * global row they were inheriting unset fields from.
 */
export const useDeleteAirwayGlobalConfig = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (sourceKind: string) => AirwayConfigService.deleteGlobal(sourceKind),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.airwayConfig.config() });
      toast.success("Airway admission policy reset");
    },
    onError: (err) => toast.error(errMessage(err, "Failed to reset airway admission policy"))
  });
};

/**
 * Create or replace a workspace's override row for a source kind
 * (`PUT /admin/airway/config/{source_kind}/workspaces/{workspace_id}`).
 * Leaves the global row (if any) untouched. Same replace-not-patch semantics
 * as {@link useUpsertAirwayGlobalConfig}.
 */
export const useUpsertAirwayWorkspaceOverride = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      sourceKind,
      workspaceId,
      body
    }: {
      sourceKind: string;
      workspaceId: string;
      body: UpsertAirwayConfigBody;
    }) => AirwayConfigService.upsertOverride(sourceKind, workspaceId, body),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.airwayConfig.config() });
      toast.success("Workspace override updated");
    },
    onError: (err) => toast.error(errMessage(err, "Failed to update workspace override"))
  });
};

/**
 * Delete a workspace's override row for a source kind
 * (`DELETE /admin/airway/config/{source_kind}/workspaces/{workspace_id}`).
 * The workspace goes back to inheriting the global row in full.
 */
export const useDeleteAirwayWorkspaceOverride = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ sourceKind, workspaceId }: { sourceKind: string; workspaceId: string }) =>
      AirwayConfigService.deleteOverride(sourceKind, workspaceId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.airwayConfig.config() });
      toast.success("Workspace override removed");
    },
    onError: (err) => toast.error(errMessage(err, "Failed to remove workspace override"))
  });
};

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { isAxiosError } from "axios";
import { toast } from "sonner";
import { AirwayDeploymentService, type AirwayDeploymentValues } from "@/services/api/airwayConfig";
import queryKeys from "../queryKey";

function errMessage(err: unknown, fallback: string): string {
  if (isAxiosError(err)) return err.response?.data?.message ?? err.message;
  if (err instanceof Error) return err.message;
  return fallback;
}

/**
 * airway's deployment (operational) tier — the configured row, what the
 * answering process actually installed, and the drift between them
 * (`GET /admin/airway/deployment-config`). Staff-only, gated on the
 * `PlatformOperate` capability — not owner-only.
 *
 * `installed` is one process's `OnceLock`, so the payload's `installed_scope`
 * names which process answered. Never render `installed` without it.
 */
export const useAirwayDeploymentConfig = () =>
  useQuery({
    queryKey: queryKeys.airwayConfig.deployment(),
    queryFn: () => AirwayDeploymentService.get()
  });

/**
 * Save the deployment tier (`PUT`). A replace, not a patch — always send all
 * ten fields; `null` clears a setting back to airway's default.
 *
 * The success toast says "on the next worker restart" deliberately. airway's
 * install is one-shot per process, so the save is durable but inert until a
 * restart, and a bare "saved" here is the exact misreading the drift indicator
 * exists to correct.
 */
export const useUpsertAirwayDeploymentConfig = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: AirwayDeploymentValues) => AirwayDeploymentService.upsert(body),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.airwayConfig.deployment() });
      toast.success("Saved — applies on the next airway worker restart");
    },
    onError: (err) => toast.error(errMessage(err, "Failed to save airway deployment config"))
  });
};

/**
 * Remove the row entirely (`DELETE`). Every setting goes back to airway's
 * built-in default — again, at the next restart, not now.
 */
export const useClearAirwayDeploymentConfig = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => AirwayDeploymentService.clear(),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.airwayConfig.deployment() });
      toast.success("Cleared — airway's defaults apply on the next worker restart");
    },
    onError: (err) => toast.error(errMessage(err, "Failed to clear airway deployment config"))
  });
};

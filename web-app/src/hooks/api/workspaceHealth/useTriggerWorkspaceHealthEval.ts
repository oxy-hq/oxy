import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import {
  type WorkspaceHealthResponse,
  WorkspaceHealthService
} from "@/services/api/workspaceHealth";
import queryKeys from "../queryKey";

/** Poll cadence + ceiling while waiting for the enqueued eval pass to land. */
const POLL_INTERVAL_MS = 2_000;
const POLL_TIMEOUT_MS = 60_000;

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const checkedAtMs = (value: string | null): number => (value ? new Date(value).getTime() : 0);

/**
 * Polls the rollup until the workspace's `checked_at` advances past `baseline`
 * (the value captured before the trigger), i.e. a fresh eval pass has been
 * persisted. Resolves `true` once it lands, `false` if the poll ceiling is hit
 * first (the eval may still complete on the fleet — we just stop waiting).
 */
const waitForFreshCheck = async (
  workspaceId: string,
  baseline: string | null
): Promise<boolean> => {
  const deadline = Date.now() + POLL_TIMEOUT_MS;
  const baselineMs = checkedAtMs(baseline);
  while (Date.now() < deadline) {
    await sleep(POLL_INTERVAL_MS);
    const data = await WorkspaceHealthService.list();
    const entry = data.workspaces.find((ws) => ws.workspace_id === workspaceId);
    if (entry && checkedAtMs(entry.checked_at) > baselineMs) {
      return true;
    }
  }
  return false;
};

/**
 * Triggers an on-demand health eval for a single workspace
 * (`POST /admin/workspace-health/{id}/eval`). The eval is offloaded to the
 * worker fleet (the endpoint returns 202 + a run id, not the row), so this waits
 * for the persisted result by polling the rollup until the workspace's
 * `checked_at` advances, then invalidates the workspace-health key group so both
 * the per-workspace Health tab and the fleet rollup refetch. `isPending` stays
 * true for the whole wait, so the trigger button keeps its disabled+spinner
 * state until the pass lands.
 */
export const useTriggerWorkspaceHealthEval = () => {
  const queryClient = useQueryClient();

  return useMutation<{ landed: boolean }, Error, string>({
    mutationFn: async (workspaceId) => {
      // Snapshot "last checked" before the eval so we can detect the new pass.
      const baseline =
        queryClient
          .getQueryData<WorkspaceHealthResponse>(queryKeys.workspaceHealth.list())
          ?.workspaces.find((ws) => ws.workspace_id === workspaceId)?.checked_at ?? null;

      await WorkspaceHealthService.trigger(workspaceId);
      const landed = await waitForFreshCheck(workspaceId, baseline);
      return { landed };
    },
    onSuccess: ({ landed }) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.workspaceHealth.all });
      if (landed) {
        toast.success("Health check complete");
      } else {
        toast.info("Health check is running; results will appear shortly.");
      }
    },
    onError: (error) => {
      // The trigger itself failed (the eval never enqueued); refresh anyway in
      // case a prior pass landed, and surface the failure.
      queryClient.invalidateQueries({ queryKey: queryKeys.workspaceHealth.all });
      console.error("Failed to run workspace health check:", error);
      toast.error("Failed to run health check", { description: error.message });
    }
  });
};

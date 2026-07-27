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
/**
 * A forced smoke run is a different order of magnitude: it pings every warehouse
 * connection, runs a measure query per topic, runs every app, and puts a question
 * to an agent. The backend's own whole-run backstop is 300s, so wait that long
 * before giving up on the poll — a ceiling shorter than the work it is waiting
 * for would report "still running" on every successful smoke run.
 */
const SMOKE_POLL_TIMEOUT_MS = 300_000;

/** What to trigger: a plain eval pass, or one that also forces the smoke probes. */
export interface TriggerEvalVars {
  workspaceId: string;
  /** Force the smoke probes to run regardless of their cadence. Default false. */
  smoke?: boolean;
}

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
  baseline: string | null,
  timeoutMs: number
): Promise<boolean> => {
  const deadline = Date.now() + timeoutMs;
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
 * (`POST /admin/workspace-health/{id}/eval`), optionally forcing the smoke probes
 * (`?smoke=true`). The eval is offloaded to the worker fleet (the endpoint
 * returns 202 + a run id, not the row), so this waits for the persisted result by
 * polling the rollup until the workspace's `checked_at` advances, then invalidates
 * the workspace-health key group so both the per-workspace Health tab and the
 * fleet rollup refetch. `isPending` stays true for the whole wait, so the trigger
 * button keeps its disabled+spinner state until the pass lands.
 *
 * Both buttons share this hook: a smoke run *is* an eval pass, and the only
 * differences are the flag, the (much longer) wait, and the copy.
 */
export const useTriggerWorkspaceHealthEval = () => {
  const queryClient = useQueryClient();

  return useMutation<{ landed: boolean; smoke: boolean }, Error, TriggerEvalVars>({
    mutationFn: async ({ workspaceId, smoke = false }) => {
      // Snapshot "last checked" before the eval so we can detect the new pass.
      const baseline =
        queryClient
          .getQueryData<WorkspaceHealthResponse>(queryKeys.workspaceHealth.list())
          ?.workspaces.find((ws) => ws.workspace_id === workspaceId)?.checked_at ?? null;

      await WorkspaceHealthService.trigger(workspaceId, smoke);
      const landed = await waitForFreshCheck(
        workspaceId,
        baseline,
        smoke ? SMOKE_POLL_TIMEOUT_MS : POLL_TIMEOUT_MS
      );
      return { landed, smoke };
    },
    onSuccess: ({ landed, smoke }) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.workspaceHealth.all });
      const label = smoke ? "Smoke test" : "Health check";
      if (landed) {
        toast.success(`${label} complete`);
      } else {
        toast.info(`${label} is running; results will appear shortly.`);
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

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CustomAppsService } from "@/services/api/customApps";
import type { BatchAppResult } from "@/types/apps";
import queryKeys from "../queryKey";

/**
 * The fleet's one batch verb: promote the newest build on every selected app
 * (`POST /customer-apps/batch/promote-latest`).
 *
 * Returns a [`BatchAppResult`] with a per-app outcome — the caller folds that into
 * one summary toast ("Promoted 4, 1 failed") rather than firing a toast per app. On
 * settle the whole custom-apps key root (`customApps.all()`) is invalidated, which
 * covers the admin registry, every app's build history, and the workspace launcher
 * lists a promotion can change.
 *
 * Batch publish / unpublish / delete hooks lived here too, with no consumer since the
 * selectable apps table was deleted; they went with it. Per-app publish, unpublish
 * and rollback are the lifecycle card's verbs (`AppLifecycle/actions.ts`).
 */
export const useBatchPromoteLatestApps = () => {
  const qc = useQueryClient();
  return useMutation<BatchAppResult, Error, string[]>({
    mutationFn: CustomAppsService.batchPromoteLatest,
    onSettled: () => {
      qc.invalidateQueries({ queryKey: queryKeys.customApps.all() });
    }
  });
};

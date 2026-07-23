import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CustomAppsService } from "@/services/api/customApps";
import type { BatchAppResult } from "@/types/apps";
import queryKeys from "../queryKey";

/**
 * Batch publish / unpublish / delete for the admin apps table.
 *
 * Each hook takes the selected app ids and returns a [`BatchAppResult`] with a
 * per-app outcome — the caller folds that into one summary toast ("Published
 * 4, 1 failed") rather than firing a toast per app. On settle we invalidate
 * the whole custom-apps key root (`customApps.all()`), which covers both the
 * admin registry and the workspace launcher lists, since publish/unpublish
 * flips an app's visibility in the owning workspace.
 */
const useBatchAppMutation = (mutationFn: (ids: string[]) => Promise<BatchAppResult>) => {
  const qc = useQueryClient();
  return useMutation<BatchAppResult, Error, string[]>({
    mutationFn,
    onSettled: () => {
      qc.invalidateQueries({ queryKey: queryKeys.customApps.all() });
    }
  });
};

export const useBatchPublishApps = () => useBatchAppMutation(CustomAppsService.batchPublish);

export const useBatchPromoteLatestApps = () =>
  useBatchAppMutation(CustomAppsService.batchPromoteLatest);

export const useBatchUnpublishApps = () => useBatchAppMutation(CustomAppsService.batchUnpublish);

export const useBatchDeleteApps = () => useBatchAppMutation(CustomAppsService.batchDelete);

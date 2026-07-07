import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CustomerAppsService } from "@/services/api/customerApps";
import type { BatchAppResult } from "@/types/apps";
import queryKeys from "../queryKey";

/**
 * Batch publish / unpublish / delete for the admin apps table.
 *
 * Each hook takes the selected app ids and returns a [`BatchAppResult`] with a
 * per-app outcome — the caller folds that into one summary toast ("Published
 * 4, 1 failed") rather than firing a toast per app. On settle we invalidate
 * both the admin registry (`customerApps.all`, the table itself) and the
 * workspace sidebar lists (`customApps.all`), since publish/unpublish flips an
 * app's visibility in the owning workspace's Custom Apps section.
 */
const useBatchAppMutation = (mutationFn: (ids: string[]) => Promise<BatchAppResult>) => {
  const qc = useQueryClient();
  return useMutation<BatchAppResult, Error, string[]>({
    mutationFn,
    onSettled: () => {
      qc.invalidateQueries({ queryKey: queryKeys.customerApps.all() });
      qc.invalidateQueries({ queryKey: queryKeys.customApps.all });
    }
  });
};

export const useBatchPublishApps = () => useBatchAppMutation(CustomerAppsService.batchPublish);

export const useBatchPromoteLatestApps = () =>
  useBatchAppMutation(CustomerAppsService.batchPromoteLatest);

export const useBatchUnpublishApps = () => useBatchAppMutation(CustomerAppsService.batchUnpublish);

export const useBatchDeleteApps = () => useBatchAppMutation(CustomerAppsService.batchDelete);

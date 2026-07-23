import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { CustomAppsService } from "@/services/api/customApps";
import queryKeys from "../queryKey";

/**
 * Versioned build history for a custom app (new publish pipeline).
 * Disabled until an id is available; empty for legacy rows.
 */
export function useAppBuilds(id: string | undefined) {
  return useQuery({
    queryKey: queryKeys.customApps.builds(id ?? ""),
    queryFn: () => CustomAppsService.listBuilds(id as string),
    enabled: !!id
  });
}

/**
 * Roll the published channel back to a retained build. Invalidates the
 * app list (published pointer changed) and this app's build history.
 */
export function useRollbackApp() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, buildId }: { id: string; buildId: string }) =>
      CustomAppsService.rollback(id, buildId),
    onSuccess: (_data, { id }) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.customApps.all() });
      queryClient.invalidateQueries({ queryKey: queryKeys.customApps.builds(id) });
    }
  });
}

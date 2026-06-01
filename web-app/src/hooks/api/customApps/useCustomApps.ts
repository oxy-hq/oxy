import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { isAxiosError } from "axios";
import { toast } from "sonner";
import { CustomAppsService } from "@/services/api/customApps";
import { CustomerAppsService } from "@/services/api/customerApps";
import queryKeys from "../queryKey";

/**
 * Published custom apps for a workspace. Drives the workspace
 * sidebar's Custom Apps section. Empty array (not error) when the
 * workspace has none — sidebar can render conditionally.
 */
export const useCustomApps = (workspaceId: string) =>
  useQuery({
    queryKey: queryKeys.customApps.list(workspaceId),
    queryFn: () => CustomAppsService.list(workspaceId),
    enabled: !!workspaceId
  });

export const usePublishApp = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: CustomerAppsService.publish,
    onSuccess: (app) => {
      qc.invalidateQueries({ queryKey: queryKeys.customerApps.all() });
      qc.invalidateQueries({ queryKey: queryKeys.customerApps.mine() });
      // Workspace-scoped list — invalidate the one for this app's workspace
      // so the sidebar picks up the new entry immediately.
      qc.invalidateQueries({ queryKey: queryKeys.customApps.list(app.project_id) });
      toast.success(`${app.name} published`);
    },
    onError: (err) => {
      const message = isAxiosError(err)
        ? (err.response?.data?.message ?? err.message)
        : err instanceof Error
          ? err.message
          : "Failed to publish";
      toast.error(message);
    }
  });
};

export const useUnpublishApp = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: CustomerAppsService.unpublish,
    onSuccess: (app) => {
      qc.invalidateQueries({ queryKey: queryKeys.customerApps.all() });
      qc.invalidateQueries({ queryKey: queryKeys.customerApps.mine() });
      qc.invalidateQueries({ queryKey: queryKeys.customApps.list(app.project_id) });
      toast.success(`${app.name} unpublished`);
    },
    onError: (err) => {
      const message = isAxiosError(err)
        ? (err.response?.data?.message ?? err.message)
        : err instanceof Error
          ? err.message
          : "Failed to unpublish";
      toast.error(message);
    }
  });
};

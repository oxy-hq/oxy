import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { isAxiosError } from "axios";
import { toast } from "sonner";
import { OxyAccessService } from "@/services/api/access";
import queryKeys from "../queryKey";

export const useOxyAccess = (workspaceId: string) =>
  useQuery({
    queryKey: queryKeys.oxyAccess.status(workspaceId),
    queryFn: () => OxyAccessService.get(workspaceId),
    enabled: !!workspaceId
  });

/**
 * Toggle the per-workspace "let Oxy build tailored apps on our data"
 * flag. Mutation accepts the desired state; the hook routes to the
 * matching enable/disable endpoint and shows a toast.
 */
export const useSetOxyAccess = (workspaceId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (enabled: boolean) => {
      if (enabled) {
        return OxyAccessService.enable(workspaceId);
      }
      await OxyAccessService.disable(workspaceId);
      return null;
    },
    onSuccess: (_, enabled) => {
      qc.invalidateQueries({ queryKey: queryKeys.oxyAccess.status(workspaceId) });
      toast.success(enabled ? "Oxy access granted" : "Oxy access revoked");
    },
    onError: (err) => {
      const message = isAxiosError(err)
        ? (err.response?.data?.message ?? err.message)
        : err instanceof Error
          ? err.message
          : "Failed to update Oxy access";
      toast.error(message);
    }
  });
};

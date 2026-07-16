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
 * Set the per-workspace Oxy-staff LOCKDOWN. `true` = lock Oxy out; `false` =
 * lift the lockdown (the default, where Oxy support can reach your apps).
 */
export const useSetOxyLockdown = (workspaceId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (locked: boolean) => {
      if (locked) {
        return OxyAccessService.lock(workspaceId);
      }
      await OxyAccessService.unlock(workspaceId);
      return null;
    },
    onSuccess: (_, locked) => {
      qc.invalidateQueries({ queryKey: queryKeys.oxyAccess.status(workspaceId) });
      toast.success(locked ? "Oxy staff locked out" : "Oxy staff access restored");
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

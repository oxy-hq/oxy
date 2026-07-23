import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CustomAppsService } from "@/services/api/customApps";
import queryKeys from "../queryKey";

export const useDeleteApp = () => {
  const qc = useQueryClient();
  return useMutation<void, Error, string>({
    mutationFn: CustomAppsService.delete,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.customApps.all() });
    }
  });
};

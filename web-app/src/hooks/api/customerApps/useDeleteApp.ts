import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CustomerAppsService } from "@/services/api/customerApps";
import queryKeys from "../queryKey";

export const useDeleteApp = () => {
  const qc = useQueryClient();
  return useMutation<void, Error, string>({
    mutationFn: CustomerAppsService.delete,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.customerApps.all() });
    }
  });
};

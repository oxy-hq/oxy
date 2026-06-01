import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CustomerAppsService } from "@/services/api/customerApps";
import type { CreateAppRequest, CustomerApp } from "@/types/apps";
import queryKeys from "../queryKey";

export const useCreateApp = () => {
  const qc = useQueryClient();
  return useMutation<CustomerApp, Error, CreateAppRequest>({
    mutationFn: CustomerAppsService.create,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.customerApps.all() });
    }
  });
};

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CustomAppsService } from "@/services/api/customApps";
import type { CreateAppRequest, CustomApp } from "@/types/apps";
import queryKeys from "../queryKey";

export const useCreateApp = () => {
  const qc = useQueryClient();
  return useMutation<CustomApp, Error, CreateAppRequest>({
    mutationFn: CustomAppsService.create,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.customApps.all() });
    }
  });
};

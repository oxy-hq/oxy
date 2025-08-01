import { AppService } from "@/services/api";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import { AppData } from "@/types/app";

const useRunAppMutation = (onSuccess: (data: AppData) => void) => {
  const queryClient = useQueryClient();
  return useMutation<AppData, Error, string>({
    mutationFn: AppService.runApp,
    onSuccess: (data: AppData, variables: string) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.app.getAppData(variables),
      });
      onSuccess(data);
    },
  });
};

export default useRunAppMutation;

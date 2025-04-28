import { service } from "@/services/service";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "./queryKey";
import { App } from "@/types/app";

const useRunAppMutation = (onSuccess: (data: App) => void) => {
  const queryClient = useQueryClient();
  return useMutation<App, Error, string>({
    mutationFn: service.runApp,
    onSuccess: (data: App, variables: string) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.app.get(variables) });
      onSuccess(data);
    },
  });
};

export default useRunAppMutation;

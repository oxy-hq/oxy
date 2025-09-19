import { AppService } from "@/services/api";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import { AppData } from "@/types/app";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

const useRunAppMutation = (onSuccess: (data: AppData) => void) => {
  const queryClient = useQueryClient();
  const { project, branchName } = useCurrentProjectBranch();
  const projectId = project.id;

  return useMutation<AppData, Error, string>({
    mutationFn: (pathb64: string) =>
      AppService.runApp(projectId, branchName, pathb64),
    onSuccess: (data: AppData, variables: string) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.app.getAppData(projectId, branchName, variables),
      });
      onSuccess(data);
    },
  });
};

export default useRunAppMutation;

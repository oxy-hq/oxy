import { useMutation, useQueryClient } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { AppService } from "@/services/api";
import type { AppData } from "@/types/app";
import queryKeys from "../queryKey";

type RunArgs = {
  pathb64: string;
  params?: Record<string, unknown>;
};

const useRunAppMutation = (onSuccess: (data: AppData) => void) => {
  const queryClient = useQueryClient();
  const { project, branchName } = useCurrentProjectBranch();
  const projectId = project.id;

  return useMutation<AppData, Error, RunArgs>({
    mutationFn: ({ pathb64, params = {} }) =>
      AppService.runApp(projectId, branchName, pathb64, params),
    onSuccess: (data: AppData, { pathb64, params = {} }) => {
      // Only invalidate the cached query when running without params (full refresh)
      if (Object.keys(params).length === 0) {
        queryClient.invalidateQueries({
          queryKey: queryKeys.app.getAppData(projectId, branchName, pathb64)
        });
      }
      onSuccess(data);
    }
  });
};

export default useRunAppMutation;

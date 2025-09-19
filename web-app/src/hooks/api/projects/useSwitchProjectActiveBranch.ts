import { ProjectService } from "@/services/api";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "../queryKey";

export const useSwitchProjectActiveBranch = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      projectId,
      branchName,
    }: {
      projectId: string;
      branchName: string;
    }) => ProjectService.switchProjectActiveBranch(projectId, branchName),
    onSuccess: (_, variables) => {
      // Invalidate project details and branches to refetch
      queryClient.invalidateQueries({
        queryKey: queryKeys.projects.item(variables.projectId),
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.projects.branches(variables.projectId),
      });
    },
  });
};

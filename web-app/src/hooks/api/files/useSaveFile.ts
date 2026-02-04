import { useMutation, useQueryClient } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { FileService } from "@/services/api";
import queryKeys from "../queryKey";

export default function useSaveFile() {
  const { project, branchName } = useCurrentProjectBranch();

  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (data: { pathb64: string; data: string }) =>
      FileService.saveFile(project.id, data.pathb64, data.data, branchName),
    onSuccess: (_, variables) => {
      queryClient.removeQueries({
        queryKey: queryKeys.file.get(project.id, variables.pathb64, branchName)
      });
      queryClient.setQueryData(
        queryKeys.file.get(project.id, branchName, variables.pathb64),
        variables.data
      );
    }
  });
}

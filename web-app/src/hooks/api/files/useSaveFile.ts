import { FileService } from "@/services/api";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export default function useSaveFile() {
  const { project, branchName } = useCurrentProjectBranch();

  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (data: { pathb64: string; data: string }) =>
      FileService.saveFile(project.id, data.pathb64, data.data, branchName),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.file.get(project.id, variables.pathb64, branchName),
      });
    },
  });
}

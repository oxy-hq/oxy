import { FileService } from "@/services/api";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export default function useDeleteFile() {
  const { project, branchName } = useCurrentProjectBranch();

  const queryClient = useQueryClient();
  return useMutation<void, Error, string>({
    mutationFn: (pathb64: string) =>
      FileService.deleteFile(project.id, pathb64, branchName),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.file.tree(project.id, branchName),
      });
    },
  });
}

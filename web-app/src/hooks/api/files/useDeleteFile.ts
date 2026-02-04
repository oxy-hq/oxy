import { useMutation, useQueryClient } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { FileService } from "@/services/api";
import queryKeys from "../queryKey";

export default function useDeleteFile() {
  const { project, branchName } = useCurrentProjectBranch();

  const queryClient = useQueryClient();
  return useMutation<void, Error, string>({
    mutationFn: (pathb64: string) => FileService.deleteFile(project.id, pathb64, branchName),
    onSuccess: (_, pathb64) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.file.tree(project.id, branchName)
      });
      queryClient.removeQueries({
        queryKey: queryKeys.file.get(project.id, branchName, pathb64)
      });
    }
  });
}

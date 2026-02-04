import { useMutation, useQueryClient } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { FileService } from "@/services/api";
import queryKeys from "../queryKey";

export default function useDeleteFolder() {
  const { project, branchName } = useCurrentProjectBranch();

  const queryClient = useQueryClient();
  return useMutation<void, Error, string>({
    mutationFn: (pathb64: string) => FileService.deleteFolder(project.id, pathb64, branchName),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.file.tree(project.id, branchName)
      });
    }
  });
}

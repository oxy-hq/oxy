import { useMutation, useQueryClient } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { FileService } from "@/services/api";
import queryKeys from "../queryKey";

export default function useRenameFile() {
  const { project, branchName } = useCurrentProjectBranch();

  const queryClient = useQueryClient();
  return useMutation<void, Error, { pathb64: string; newName: string }>({
    mutationFn: ({ pathb64, newName }) =>
      FileService.renameFile(project.id, pathb64, newName, branchName),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.file.tree(project.id, branchName)
      });
    }
  });
}

import { FileService } from "@/services/api";
import { useMutation, useQueryClient } from "@tanstack/react-query";

export default function useRenameFolder() {
  const queryClient = useQueryClient();
  return useMutation<void, Error, { pathb64: string; newName: string }>({
    mutationFn: ({ pathb64, newName }) =>
      FileService.renameFolder(pathb64, newName),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["fileTree"] });
    },
  });
}

import { FileService } from "@/services/api";
import { useMutation, useQueryClient } from "@tanstack/react-query";

export default function useDeleteFolder() {
  const queryClient = useQueryClient();
  return useMutation<void, Error, string>({
    mutationFn: (pathb64: string) => FileService.deleteFolder(pathb64),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["fileTree"] });
    },
  });
}

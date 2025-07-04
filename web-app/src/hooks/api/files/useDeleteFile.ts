import { FileService } from "@/services/api";
import { useMutation, useQueryClient } from "@tanstack/react-query";

export default function useDeleteFile() {
  const queryClient = useQueryClient();
  return useMutation<void, Error, string>({
    mutationFn: (pathb64: string) => FileService.deleteFile(pathb64),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["fileTree"] });
    },
  });
}

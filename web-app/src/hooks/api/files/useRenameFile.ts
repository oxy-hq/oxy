import { service } from "@/services/service";
import { useMutation, useQueryClient } from "@tanstack/react-query";

export default function useRenameFile() {
  const queryClient = useQueryClient();
  return useMutation<void, Error, { pathb64: string; newName: string }>({
    mutationFn: ({ pathb64, newName }) => service.renameFile(pathb64, newName),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["fileTree"] });
    },
  });
}

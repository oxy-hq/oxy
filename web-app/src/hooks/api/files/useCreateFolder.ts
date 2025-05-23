import { service } from "@/services/service";
import { useMutation, useQueryClient } from "@tanstack/react-query";

export default function useCreateFolder() {
  const queryClient = useQueryClient();
  return useMutation<void, Error, string>({
    mutationFn: (pathb64: string) => service.createFolder(pathb64),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["fileTree"] });
    },
  });
}

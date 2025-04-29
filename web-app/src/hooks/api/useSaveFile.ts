import { service } from "@/services/service";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "./queryKey";

export default function useSaveFile() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (data: { pathb64: string; data: string }) =>
      service.saveFile(data.pathb64, data.data),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.file.get(variables.pathb64),
      });
    },
  });
}

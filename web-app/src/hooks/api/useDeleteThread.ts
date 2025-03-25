import { service } from "@/services/service";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "./queryKey";

const useDeleteThread = () => {
  const queryClient = useQueryClient();
  return useMutation<void, Error, string>({
    mutationFn: service.deleteThread,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.thread.list() });
    },
  });
};

export default useDeleteThread;

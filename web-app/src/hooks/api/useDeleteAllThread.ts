import { service } from "@/services/service";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "./queryKey";

const useDeleteAllThread = () => {
  const queryClient = useQueryClient();
  return useMutation<void, Error, void>({
    mutationFn: service.deleteAllThread,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.thread.list() });
    },
  });
};

export default useDeleteAllThread;

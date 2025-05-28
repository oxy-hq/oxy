import { service } from "@/services/service";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "./queryKey";

const useDeleteAllThread = () => {
  const queryClient = useQueryClient();
  return useMutation<void, Error, void>({
    mutationFn: service.deleteAllThread,
    onSuccess: () => {
      // Invalidate all thread queries (all pages and individual threads)
      queryClient.invalidateQueries({
        queryKey: queryKeys.thread.all,
        type: "all",
      });
    },
  });
};

export default useDeleteAllThread;

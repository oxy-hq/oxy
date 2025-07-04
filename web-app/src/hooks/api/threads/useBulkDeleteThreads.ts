import { ThreadService } from "@/services/api";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "../queryKey";

const useBulkDeleteThreads = () => {
  const queryClient = useQueryClient();
  return useMutation<void, Error, string[]>({
    mutationFn: ThreadService.bulkDeleteThreads,
    onSuccess: () => {
      // Invalidate all thread list queries (all pages)
      queryClient.invalidateQueries({
        queryKey: queryKeys.thread.all,
        type: "all",
      });
    },
  });
};

export default useBulkDeleteThreads;

import { service } from "@/services/service";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "./queryKey";
import { ThreadCreateRequest, ThreadItem } from "@/types/chat";

const useThreadMutation = (onSuccess: (data: ThreadItem) => void) => {
  const queryClient = useQueryClient();
  return useMutation<ThreadItem, Error, ThreadCreateRequest>({
    mutationFn: service.createThread,
    onSuccess: (data: ThreadItem) => {
      // Invalidate all thread list queries (all pages)
      queryClient.invalidateQueries({
        queryKey: queryKeys.thread.all,
      });
      onSuccess(data);
    },
  });
};

export default useThreadMutation;

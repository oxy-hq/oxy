import { service } from "@/services/service";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "./queryKey";
import { ThreadCreateRequest, ThreadItem } from "@/types/chat";

const useThreadMutation = (onSuccess: (data: ThreadItem) => void) => {
  const queryClient = useQueryClient();
  return useMutation<ThreadItem, Error, ThreadCreateRequest>({
    mutationFn: service.createThread,
    onSuccess: (data: ThreadItem) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.thread.list() });
      onSuccess(data);
    },
  });
};

export default useThreadMutation;

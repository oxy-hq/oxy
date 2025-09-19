import { ThreadService } from "@/services/api";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import { ThreadCreateRequest, ThreadItem } from "@/types/chat";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

const useThreadMutation = (onSuccess: (data: ThreadItem) => void) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const queryClient = useQueryClient();
  return useMutation<ThreadItem, Error, ThreadCreateRequest>({
    mutationFn: (request) => ThreadService.createThread(projectId, request),
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

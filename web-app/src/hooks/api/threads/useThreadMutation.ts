import { useMutation, useQueryClient } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ThreadService } from "@/services/api";
import type { ThreadCreateRequest, ThreadItem } from "@/types/chat";
import queryKeys from "../queryKey";

const useThreadMutation = (onSuccess: (data: ThreadItem) => void) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const queryClient = useQueryClient();
  return useMutation<ThreadItem, Error, ThreadCreateRequest>({
    mutationFn: (request) => ThreadService.createThread(projectId, request),
    onSuccess: (data: ThreadItem) => {
      // Invalidate all thread list queries (all pages)
      queryClient.invalidateQueries({
        queryKey: queryKeys.thread.all
      });
      onSuccess(data);
    }
  });
};

export default useThreadMutation;

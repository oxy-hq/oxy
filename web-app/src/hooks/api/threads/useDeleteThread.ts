import { useMutation, useQueryClient } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ThreadService } from "@/services/api";
import queryKeys from "../queryKey";

const useDeleteThread = () => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const queryClient = useQueryClient();
  return useMutation<void, Error, string>({
    mutationFn: (threadId) => ThreadService.deleteThread(projectId, threadId),
    onSuccess: () => {
      // Invalidate all thread list queries (all pages)
      queryClient.invalidateQueries({
        queryKey: queryKeys.thread.all,
        type: "all"
      });
    }
  });
};

export default useDeleteThread;

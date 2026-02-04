import { useMutation, useQueryClient } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ThreadService } from "@/services/api";
import queryKeys from "../queryKey";

const useBulkDeleteThreads = () => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const queryClient = useQueryClient();
  return useMutation<void, Error, string[]>({
    mutationFn: (threadIds) => ThreadService.bulkDeleteThreads(projectId, threadIds),
    onSuccess: () => {
      // Invalidate all thread list queries (all pages)
      queryClient.invalidateQueries({
        queryKey: queryKeys.thread.all,
        type: "all"
      });
    }
  });
};

export default useBulkDeleteThreads;

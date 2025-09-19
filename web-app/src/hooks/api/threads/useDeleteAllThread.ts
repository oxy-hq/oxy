import { ThreadService } from "@/services/api";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

const useDeleteAllThread = () => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const queryClient = useQueryClient();
  return useMutation<void, Error, void>({
    mutationFn: () => ThreadService.deleteAllThreads(projectId),
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

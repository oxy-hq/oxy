import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ThreadService } from "@/services/api";
import type { ThreadsResponse } from "@/types/chat";
import queryKeys from "../queryKey";

const useThreads = (
  page: number = 1,
  limit: number = 100,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false
) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  return useQuery<ThreadsResponse, Error>({
    queryKey: queryKeys.thread.list(projectId, page, limit),
    queryFn: () => ThreadService.listThreads(projectId, page, limit),
    enabled,
    refetchOnWindowFocus,
    refetchOnMount
  });
};

export default useThreads;

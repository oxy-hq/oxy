import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ThreadService } from "@/services/api";
import type { ThreadItem } from "@/types/chat";
import queryKeys from "../queryKey";

const useThread = (
  threadId: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
  projectIdOverride?: string
) => {
  const { project } = useCurrentProjectBranch();
  const projectId = projectIdOverride ?? project?.id ?? "00000000-0000-0000-0000-000000000000";
  return useQuery<ThreadItem, Error>({
    queryKey: queryKeys.thread.item(projectId, threadId),
    queryFn: () => ThreadService.getThread(projectId, threadId),
    enabled,
    refetchOnWindowFocus,
    refetchOnMount
  });
};

export default useThread;

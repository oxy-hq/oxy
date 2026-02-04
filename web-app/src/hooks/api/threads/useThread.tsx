import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ThreadService } from "@/services/api";
import type { ThreadItem } from "@/types/chat";
import queryKeys from "../queryKey";

const useThread = (
  threadId: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false
) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  return useQuery<ThreadItem, Error>({
    queryKey: queryKeys.thread.item(projectId, threadId),
    queryFn: () => ThreadService.getThread(projectId, threadId),
    enabled,
    refetchOnWindowFocus,
    refetchOnMount
  });
};

export default useThread;

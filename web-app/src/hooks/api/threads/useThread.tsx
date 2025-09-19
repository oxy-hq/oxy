import { ThreadService } from "@/services/api";
import { useQuery } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import { ThreadItem } from "@/types/chat";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

const useThread = (
  threadId: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  return useQuery<ThreadItem, Error>({
    queryKey: queryKeys.thread.item(projectId, threadId),
    queryFn: () => ThreadService.getThread(projectId, threadId),
    enabled,
    refetchOnWindowFocus,
    refetchOnMount,
  });
};

export default useThread;

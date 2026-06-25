import { keepPreviousData, useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ThreadService } from "@/services/api";
import type { ThreadsResponse } from "@/types/chat";
import queryKeys from "../queryKey";

interface UseThreadsOptions {
  page?: number;
  limit?: number;
  /** Case-insensitive title/input filter, applied server-side. */
  search?: string;
  enabled?: boolean;
  refetchOnWindowFocus?: boolean;
  refetchOnMount?: boolean | "always";
}

const useThreads = ({
  page = 1,
  limit = 100,
  search,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount = false
}: UseThreadsOptions = {}) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const trimmedSearch = search?.trim() || undefined;
  return useQuery<ThreadsResponse, Error>({
    queryKey: queryKeys.thread.list(projectId, page, limit, trimmedSearch),
    queryFn: () => ThreadService.listThreads(projectId, page, limit, trimmedSearch),
    enabled,
    refetchOnWindowFocus,
    refetchOnMount,
    // Keep the previous page/results visible while a new search or "show more"
    // request is in flight — no flash to empty between keystrokes.
    placeholderData: keepPreviousData
  });
};

export default useThreads;

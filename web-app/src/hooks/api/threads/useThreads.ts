import { ThreadService } from "@/services/api";
import { useQuery } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import { ThreadsResponse } from "@/types/chat";

const useThreads = (
  page: number = 1,
  limit: number = 100,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) =>
  useQuery<ThreadsResponse, Error>({
    queryKey: queryKeys.thread.list(page, limit),
    queryFn: () => ThreadService.listThreads(page, limit),
    enabled,
    refetchOnWindowFocus,
    refetchOnMount,
  });

export default useThreads;

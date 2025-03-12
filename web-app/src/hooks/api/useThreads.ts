import { service } from "@/services/service";
import { useQuery } from "@tanstack/react-query";
import queryKeys from "./queryKey";
import { ThreadItem } from "@/types/chat";

const useThreads = (
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) =>
  useQuery<ThreadItem[], Error>({
    queryKey: queryKeys.thread.list(),
    queryFn: service.listThreads,
    enabled,
    refetchOnWindowFocus,
    refetchOnMount,
  });

export default useThreads;

import { service } from "@/services/service";
import { useQuery } from "@tanstack/react-query";
import queryKeys from "./queryKey";
import { ThreadItem } from "@/types/chat";

const useThread = (
  threadId: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) =>
  useQuery<ThreadItem, Error>({
    queryKey: queryKeys.thread.item(threadId),
    queryFn: () => service.getThread(threadId),
    enabled,
    refetchOnWindowFocus,
    refetchOnMount,
  });

export default useThread;

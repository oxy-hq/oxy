import { useQuery, UseQueryResult } from "@tanstack/react-query";
import { service } from "@/services/service";
import queryKeys from "./queryKey";
import { Message } from "@/types/chat";

export const useChatMessages = (
  agentPath = "",
  enabled = true,
): UseQueryResult<Message[]> => {
  return useQuery({
    queryKey: queryKeys.conversation.messages(agentPath),
    queryFn: async () => {
      try {
        const res = await service.listChatMessages(agentPath);
        return res;
      } catch (error) {
        console.error(error);
        return [];
      }
    },
    enabled,
  });
};

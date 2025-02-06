import { useQuery } from "@tanstack/react-query";

import { apiClient } from "@/services/axios";

import queryKeys from "./queryKey";

export const useConversations = (enabled = true) => {
  return useQuery({
    queryKey: queryKeys.conversation.list(),
    queryFn: async () => {
      const response = await apiClient.get("/conversations");
      return response.data.conversations;
    },
    enabled,
  });
};

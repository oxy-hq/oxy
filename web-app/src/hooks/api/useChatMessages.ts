import { useQuery } from "@tanstack/react-query";
import axios from "axios";

import { apiClient } from "@/services/axios";

import queryKeys from "./queryKey";

export const useChatMessages = (agentPath = "", enabled = true) => {
  return useQuery({
    queryKey: queryKeys.conversation.messages(agentPath),
    queryFn: async () => {
      try {
        const response = await apiClient.get("/conversation/" + agentPath);
        return response.data.messages;
      } catch (error) {
        if (axios.isAxiosError(error) && error.response?.status === 404) {
          return [];
        }
        throw error;
      }
    },
    enabled,
  });
};

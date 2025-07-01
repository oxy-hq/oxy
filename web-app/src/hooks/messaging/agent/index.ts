import useAgentThreadStore from "@/stores/useAgentThread";
import { useMessaging } from "../core/useMessaging";
import { AgentMessageSender } from "./sender";

const useAskAgent = () => {
  const { getAgentThread, setIsLoading, setMessages } = useAgentThreadStore();

  const threadStoreAdapter = {
    getThread: (threadId: string) => getAgentThread(threadId),
    setIsLoading,
    setMessages,
  };

  const messageSender = new AgentMessageSender();
  const { sendMessage } = useMessaging(threadStoreAdapter, messageSender);

  return { sendMessage };
};

export default useAskAgent;

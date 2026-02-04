import useTaskThreadStore from "@/stores/useTaskThread";
import { useMessaging } from "../core/useMessaging";
import { TaskMessageSender } from "./sender";

const useAskTask = () => {
  const { getTaskThread, setFilePath, setIsLoading, setMessages } = useTaskThreadStore();

  const threadStoreAdapter = {
    getThread: (threadId: string) => getTaskThread(threadId),
    setIsLoading,
    setMessages,
    setFilePath
  };

  const messageSender = new TaskMessageSender();
  const { sendMessage } = useMessaging(threadStoreAdapter, messageSender);

  return { sendMessage };
};

export default useAskTask;

import MessageInput from "@/components/MessageInput";
import useSendMessageMutation from "@/hooks/api/sendMessageMutation";
import { service } from "@/services/service";
import { Message, ThreadItem, MessageItem } from "@/types/chat";
import { useCallback, useEffect, useRef, useState } from "react";
import dayjs from "dayjs";
import relativeTime from "dayjs/plugin/relativeTime";
import MessageHistory from "./components/MessageHistory";
import ThreadHeader from "./components/ThreadHeader";
import StreamingMessage from "./components/StreamingMessage";

// Initialize dayjs plugins
dayjs.extend(relativeTime);

// Custom hook for message management
const useThreadMessages = (threadId: string) => {
  const [messageHistory, setMessageHistory] = useState<MessageItem[]>([]);

  const fetchMessages = useCallback(async () => {
    try {
      const messages = await service.getThreadMessages(threadId);
      setMessageHistory(messages);
    } catch (error) {
      console.error("Failed to fetch message history:", error);
    }
  }, [threadId]);

  useEffect(() => {
    fetchMessages();
  }, [fetchMessages]);

  return { messageHistory, setMessageHistory, fetchMessages };
};

// Main component
const AgentThread = ({ thread }: { thread: ThreadItem }) => {
  const [message, setMessage] = useState<Message>({
    content: "",
    references: [],
    steps: [],
    isUser: false,
    isStreaming: false,
  });

  const [followUpQuestion, setFollowUpQuestion] = useState("");
  const hasRun = useRef(false);
  const messagesContainerRef = useRef<HTMLDivElement>(null);

  const { messageHistory, setMessageHistory, fetchMessages } =
    useThreadMessages(thread.id);

  // Calculate total message count (user + agent messages)
  const totalMessageCount = messageHistory.length;
  const shouldShowWarning = totalMessageCount >= 10;

  const { sendMessage, isLoading } = useSendMessageMutation({
    threadId: thread.id,
    onStreamingMessage: setMessage,
    onMessageSent: fetchMessages,
    onMessagesUpdated: setMessageHistory,
  });

  // Auto-scroll effect
  useEffect(() => {
    if (messagesContainerRef.current) {
      messagesContainerRef.current.scrollTop =
        messagesContainerRef.current.scrollHeight;
    }
  }, [messageHistory, message]);

  // Initial message handling
  useEffect(() => {
    if (hasRun.current) return;

    if (messageHistory.length === 1) {
      sendMessage(null);
      hasRun.current = true;
    }
  }, [sendMessage, messageHistory]);

  const handleSendMessage = useCallback(() => {
    if (!followUpQuestion.trim() || isLoading) return;
    sendMessage(followUpQuestion);
    setFollowUpQuestion("");
  }, [followUpQuestion, isLoading, sendMessage]);

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <ThreadHeader thread={thread} />

      <div
        ref={messagesContainerRef}
        className="overflow-y-auto p-4 [scrollbar-gutter:stable_both-edges] customScrollbar flex-1 flex items-center w-full justify-center"
      >
        <div className="flex-1 max-w-page-content w-full h-full">
          {thread && (
            <>
              <MessageHistory messages={messageHistory} />
              <StreamingMessage message={message} />
            </>
          )}
        </div>
      </div>

      <div className="flex flex-col p-4 gap-1 pt-0 max-w-page-content mx-auto w-full">
        <MessageInput
          value={followUpQuestion}
          onChange={setFollowUpQuestion}
          onSend={handleSendMessage}
          disabled={isLoading}
          showWarning={shouldShowWarning}
          isLoading={message.isStreaming || isLoading}
        />
      </div>
    </div>
  );
};

export default AgentThread;

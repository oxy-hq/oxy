import ArtifactPanel from "@/components/ArtifactPanel";
import { Separator } from "@/components/ui/shadcn/separator";
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
import { Artifact } from "@/services/mock";

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
  const [selectedArtifactIds, setSelectedArtifactIds] = useState<string[]>([]);
  const onArtifactClick = useCallback(
    (id: string) => {
      setSelectedArtifactIds([id]);
    },
    [setSelectedArtifactIds],
  );
  const [message, setMessage] = useState<Message>({
    content: "",
    references: [],
    steps: [],
    isUser: false,
    isStreaming: false,
  });
  const [artifactStreamingData, setArtifactStreamingData] = useState<{
    [key: string]: Artifact;
  }>({});

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
    onStreamingArtifact: setArtifactStreamingData,
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

  const handleClose = () => {
    setSelectedArtifactIds([]);
  };

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <ThreadHeader thread={thread} />

      <div
        ref={messagesContainerRef}
        className="overflow-hidden flex-1 flex items-center w-full justify-center"
      >
        <div className="flex-1 w-full h-full overflow-hidden flex flex-col gap-4">
          <div className="flex-1 w-full customScrollbar overflow-auto">
            <div className="max-w-[742px] w-full p-4 mx-auto">
              {thread && (
                <>
                  <MessageHistory
                    messages={messageHistory}
                    onArtifactClick={onArtifactClick}
                  />
                  <StreamingMessage
                    message={message}
                    onArtifactClick={onArtifactClick}
                  />
                </>
              )}
            </div>
          </div>

          <div className="flex flex-col p-4 gap-1 pt-0 max-w-[742px] mx-auto w-full">
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

        {!!selectedArtifactIds.length && (
          <>
            <Separator orientation="vertical" />
            <div className="flex-1 h-full overflow-hidden">
              <ArtifactPanel
                selectedArtifactIds={selectedArtifactIds}
                artifactStreamingData={artifactStreamingData}
                onClose={handleClose}
                setSelectedArtifactIds={setSelectedArtifactIds}
              />
            </div>
          </>
        )}
      </div>
    </div>
  );
};

export default AgentThread;

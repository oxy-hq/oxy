import PageHeader from "@/components/PageHeader";
import MessageInput from "@/components/MessageInput";
import { Separator } from "@/components/ui/shadcn/separator";
import queryKeys from "@/hooks/api/queryKey";
import EditorTab from "@/pages/thread/task/EditorTab";
import MessageHistory from "@/pages/thread/agent/components/MessageHistory";
import StreamingMessage from "@/pages/thread/agent/components/StreamingMessage";
import { service } from "@/services/service";
import { ThreadItem, MessageItem, Message } from "@/types/chat";
import { useQueryClient } from "@tanstack/react-query";
import { FileCheck2 } from "lucide-react";
import { useRef, useCallback } from "react";
import { useEffect } from "react";
import { useState } from "react";

const TaskThread = ({ thread }: { thread: ThreadItem }) => {
  const queryClient = useQueryClient();

  const [isLoading, setIsLoading] = useState(false);
  const [filePath, setFilePath] = useState<string | undefined>(thread.source);
  const [followUpQuestion, setFollowUpQuestion] = useState("");
  const [messageHistory, setMessageHistory] = useState<MessageItem[]>([]);
  const [message, setMessage] = useState<Message>({
    content: "",
    references: [],
    steps: [],
    isUser: false,
    isStreaming: false,
  });
  const hasRun = useRef(false);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const shouldShowWarning = messageHistory.length > 10;

  // Fetch message history
  const fetchMessages = useCallback(async () => {
    try {
      const messages = await service.getThreadMessages(thread.id);
      setMessageHistory(messages);
    } catch (error) {
      console.error("Failed to fetch message history:", error);
    }
  }, [thread.id]);

  useEffect(() => {
    fetchMessages();
  }, [fetchMessages]);

  useEffect(() => {
    if (hasRun.current) {
      return;
    }

    if (messageHistory.length !== 1) return;

    hasRun.current = true;
    setIsLoading(true);
    // eslint-disable-next-line promise/catch-or-return
    service
      .askTask(thread.id, null, (answer) => {
        setMessage((prev) => ({
          ...prev,
          content: prev.content + answer.content,
          isStreaming: true,
        }));

        if (answer.file_path) {
          setFilePath(answer.file_path);
        }
      })
      .then(() => {
        fetchMessages();
        return null;
      })
      .finally(() => {
        setIsLoading(false);
        setMessage((prev) => ({
          ...prev,
          content: "",
          isStreaming: false,
        }));
        queryClient.invalidateQueries({
          queryKey: queryKeys.thread.list(),
          type: "all",
        });
      });
  }, [queryClient, messageHistory, thread, fetchMessages]);

  const handleSendMessage = useCallback(async () => {
    if (!followUpQuestion.trim() || isLoading) return;

    setIsLoading(true);
    setMessage((prev) => ({ ...prev, content: "", isStreaming: true }));

    try {
      await service.askTask(thread.id, followUpQuestion, (answer) => {
        setMessage((prev) => ({
          ...prev,
          content: prev.content + answer.content,
          isStreaming: true,
        }));

        if (answer.file_path) {
          setFilePath(answer.file_path);
        }
      });

      fetchMessages();
    } finally {
      setIsLoading(false);
      setMessage((prev) => ({ ...prev, isStreaming: false }));
      setFollowUpQuestion("");
    }
  }, [followUpQuestion, isLoading, thread.id, fetchMessages]);

  // Auto-scroll effect
  useEffect(() => {
    if (messagesContainerRef.current) {
      messagesContainerRef.current.scrollTop =
        messagesContainerRef.current.scrollHeight;
    }
  }, [messageHistory, message]);

  const filePathB64 = filePath ? btoa(filePath) : undefined;

  return (
    <div className="flex flex-col h-full">
      <PageHeader className="border-b-1 border-border items-center">
        <div className="p-2 flex items-center justify-center flex-1 h-full">
          <div className="flex gap-1 items-center text-muted-foreground">
            <FileCheck2 className="w-4 h-4 min-w-4 min-h-4" />
            <p className="text-sm break-all">Builder</p>
          </div>
          <div className="px-4 h-full flex items-stretch">
            <Separator orientation="vertical" />
          </div>

          <p className="text-sm text-base-foreground">{thread?.title}</p>
        </div>
      </PageHeader>

      <div className="flex flex-1 overflow-hidden">
        <div className="flex-1 flex flex-col h-full">
          <div className="flex flex-col flex-1 w-full max-w-page-content p-4 mx-auto h-full">
            <div
              ref={messagesContainerRef}
              className="flex flex-col flex-1 [scrollbar-gutter:stable_both-edges] overflow-y-auto customScrollbar"
            >
              <MessageHistory messages={messageHistory} />
              <StreamingMessage message={message} />
            </div>

            <div className="p-6 pt-0">
              <MessageInput
                value={followUpQuestion}
                onChange={setFollowUpQuestion}
                onSend={handleSendMessage}
                disabled={isLoading}
                isLoading={message.isStreaming || isLoading}
                showWarning={shouldShowWarning}
              />
            </div>
          </div>
        </div>
        <div className="border-l flex-1 h-full">
          <EditorTab pathb64={filePathB64} />
        </div>
      </div>
    </div>
  );
};

export default TaskThread;

import MessageInput from "@/components/MessageInput";
import EditorTab from "@/pages/thread/task/EditorTab";
import Messages from "@/pages/thread/messages";
import { ThreadService } from "@/services/api";
import { ThreadItem } from "@/types/chat";
import { useRef, useCallback } from "react";
import { useEffect } from "react";
import { useState } from "react";
import useTaskThreadStore from "@/stores/useTaskThread";
import useAskTask from "@/hooks/messaging/task";
import Header from "./Header";
import ProcessingWarning from "../ProcessingWarning";
import { toast } from "sonner";

const TaskThread = ({
  thread,
  refetchThread,
}: {
  thread: ThreadItem;
  refetchThread: () => void;
}) => {
  const isInitialLoad = useRef(false);
  const { getTaskThread, setMessages, setFilePath } = useTaskThreadStore();
  const taskThread = getTaskThread(thread.id);
  const bottomRef = useRef<HTMLDivElement>(null);
  const { messages, isLoading, filePath } = taskThread;

  const { sendMessage } = useAskTask();

  const [followUpQuestion, setFollowUpQuestion] = useState("");
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const shouldShowWarning = messages.length > 10;

  useEffect(() => {
    if (thread.source && filePath !== thread.source) {
      setFilePath(thread.id, thread.source);
    }
  }, [filePath, setFilePath, thread]);

  const fetchMessages = useCallback(async () => {
    try {
      const messages = await ThreadService.getThreadMessages(thread.id);
      setMessages(thread.id, messages);
      setFilePath(thread.id, thread.source);
    } catch (error) {
      console.error("Failed to fetch message history:", error);
    }
  }, [setFilePath, setMessages, thread.id, thread.source]);

  const streamingMessage = messages.find((message) => message.isStreaming);

  useEffect(() => {
    const behavior = streamingMessage?.content ? "instant" : "smooth";
    bottomRef.current?.scrollIntoView({ behavior });
  }, [messages, streamingMessage?.content]);

  useEffect(() => {
    if (messages.length > 0 || isLoading) return;

    if (isInitialLoad.current) return;
    isInitialLoad.current = true;
    fetchMessages();
  }, [fetchMessages, isLoading, messages.length]);

  const handleSendMessage = useCallback(async () => {
    if (!followUpQuestion.trim() || isLoading) return;

    await sendMessage(followUpQuestion, thread.id);
    setFollowUpQuestion("");
  }, [followUpQuestion, isLoading, sendMessage, thread.id]);

  useEffect(() => {
    if (messagesContainerRef.current) {
      messagesContainerRef.current.scrollTop =
        messagesContainerRef.current.scrollHeight;
    }
  }, [messages]);

  const filePathB64 = filePath ? btoa(filePath) : undefined;

  const onStop = useCallback(() => {
    ThreadService.stopThread(thread.id)
      // eslint-disable-next-line promise/always-return
      .then(() => {
        refetchThread();
        fetchMessages();
      })
      .catch((error) => {
        toast.error(`Failed to stop thread: ${error.message}`);
        console.error("Failed to stop thread:", error);
      });
  }, [fetchMessages, refetchThread, thread.id]);

  return (
    <div className="flex flex-col h-full">
      <Header thread={thread} />
      <div className="flex flex-1 overflow-hidden">
        <div className="flex-1 flex flex-col h-full">
          <div className="flex flex-col flex-1 w-full py-4 h-full">
            <div
              ref={messagesContainerRef}
              className="flex flex-col flex-1 [scrollbar-gutter:stable_both-edges] overflow-y-auto customScrollbar w-full"
            >
              <Messages messages={messages} />
              <div ref={bottomRef} />
            </div>

            <div className="p-6 pt-0 max-w-page-content mx-auto w-full">
              <ProcessingWarning
                threadId={thread.id}
                isLoading={isLoading}
                onRefresh={() => {
                  fetchMessages();
                  refetchThread();
                }}
              />
              <MessageInput
                value={followUpQuestion}
                onChange={setFollowUpQuestion}
                onSend={handleSendMessage}
                disabled={isLoading || thread.is_processing}
                isLoading={isLoading || thread.is_processing}
                showWarning={shouldShowWarning}
                onStop={onStop}
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

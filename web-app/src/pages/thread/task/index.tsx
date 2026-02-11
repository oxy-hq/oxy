import { ArrowDown } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import MessageInput from "@/components/MessageInput";
import { Button } from "@/components/ui/shadcn/button";
import useAskTask from "@/hooks/messaging/task";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { useSmartScroll } from "@/hooks/useSmartScroll";
import { encodeBase64 } from "@/libs/encoding";
import Messages from "@/pages/thread/messages";
import EditorTab from "@/pages/thread/task/EditorTab";
import { ThreadService } from "@/services/api";
import useTaskThreadStore from "@/stores/useTaskThread";
import type { ThreadItem } from "@/types/chat";
import ProcessingWarning from "../ProcessingWarning";
import Header from "./Header";

const MESSAGES_WARNING_THRESHOLD = 10;

const TaskThread = ({
  thread,
  refetchThread
}: {
  thread: ThreadItem;
  refetchThread: () => void;
}) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  const isInitialLoad = useRef(false);
  const { getTaskThread, setMessages, setFilePath } = useTaskThreadStore();
  const taskThread = getTaskThread(thread.id);
  const { messages, isLoading, filePath } = taskThread;

  const { sendMessage } = useAskTask();

  const [followUpQuestion, setFollowUpQuestion] = useState("");

  const { scrollContainerRef, bottomRef, isAtBottom, scrollToBottom } = useSmartScroll({
    messages
  });

  const isThreadBusy = isLoading || thread.is_processing;
  const shouldShowWarning = messages.length > MESSAGES_WARNING_THRESHOLD;

  useEffect(() => {
    if (thread.source && filePath !== thread.source) {
      setFilePath(thread.id, thread.source);
    }
  }, [filePath, setFilePath, thread]);

  // Initial message loading
  useEffect(() => {
    if (messages.length > 0 || isLoading) return;
    if (isInitialLoad.current) return;

    isInitialLoad.current = true;

    const fetchMessages = async () => {
      try {
        const messages = await ThreadService.getThreadMessages(project.id, thread.id);
        setMessages(thread.id, messages);
        setFilePath(thread.id, thread.source);
      } catch (error) {
        console.error("Failed to fetch message history:", error);
      }
    };

    fetchMessages();
  }, [isLoading, messages.length, project.id, thread.id, thread.source, setMessages, setFilePath]);

  const handleSendMessage = async () => {
    if (!followUpQuestion.trim() || isLoading) return;

    scrollToBottom();
    await sendMessage(followUpQuestion, thread.id);
    setFollowUpQuestion("");
  };

  const handleRefresh = async () => {
    try {
      const messages = await ThreadService.getThreadMessages(project.id, thread.id);
      setMessages(thread.id, messages);
      setFilePath(thread.id, thread.source);
    } catch (error) {
      console.error("Failed to fetch message history:", error);
    }
    refetchThread();
  };

  const filePathB64 = filePath ? encodeBase64(filePath) : undefined;

  const onStop = async () => {
    try {
      await ThreadService.stopThread(projectId, thread.id);
      refetchThread();

      // Re-fetch messages after stopping
      const messages = await ThreadService.getThreadMessages(project.id, thread.id);
      setMessages(thread.id, messages);
      setFilePath(thread.id, thread.source);
    } catch (error) {
      toast.error(`Failed to stop thread: ${(error as Error).message}`);
      console.error("Failed to stop thread:", error);
    }
  };

  return (
    <div className='flex h-full flex-col'>
      <Header thread={thread} />
      <div className='flex flex-1 overflow-hidden'>
        <div className='flex h-full flex-1 flex-col overflow-hidden'>
          <div className='relative w-full flex-1 overflow-hidden'>
            <div
              ref={scrollContainerRef}
              className='customScrollbar h-full w-full overflow-y-auto py-4 [scrollbar-gutter:stable_both-edges]'
            >
              <Messages messages={messages} />
              <div ref={bottomRef} />
            </div>
            {!isAtBottom && (
              <Button
                variant='outline'
                size='icon'
                onClick={scrollToBottom}
                className='absolute bottom-2 left-1/2 z-10 -translate-x-1/2 rounded-full transition-all'
                aria-label='Scroll to latest'
              >
                <ArrowDown />
              </Button>
            )}
          </div>

          <div className='mx-auto w-full max-w-page-content flex-shrink-0 p-6 pt-0'>
            <ProcessingWarning
              threadId={thread.id}
              isLoading={isLoading}
              onRefresh={handleRefresh}
            />
            <MessageInput
              value={followUpQuestion}
              onChange={setFollowUpQuestion}
              onSend={handleSendMessage}
              disabled={isThreadBusy}
              isLoading={isThreadBusy}
              showWarning={shouldShowWarning}
              onStop={onStop}
            />
          </div>
        </div>
        <div className='h-full flex-1 border-l'>
          <EditorTab pathb64={filePathB64} />
        </div>
      </div>
    </div>
  );
};

export default TaskThread;

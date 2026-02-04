import { ArrowDown } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import MessageInput from "@/components/MessageInput";
import { Button } from "@/components/ui/shadcn/button";
import useAskAgent from "@/hooks/messaging/agent";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { useSmartScroll } from "@/hooks/useSmartScroll";
import Messages from "@/pages/thread/messages";
import { ThreadService } from "@/services/api";
import useAgentThreadStore from "@/stores/useAgentThread";
import type { Message, ThreadItem } from "@/types/chat";
import ProcessingWarning from "../ProcessingWarning";
import ArtifactPanelContainer from "./components/ArtifactPanelContainer";
import ThreadHeader from "./components/ThreadHeader";

const MESSAGES_WARNING_THRESHOLD = 10;

interface AgentThreadProps {
  thread: ThreadItem;
  refetchThread: () => void;
}

const AgentThread = ({ thread, refetchThread }: AgentThreadProps) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const isInitialLoad = useRef(false);
  const { getAgentThread, setMessages } = useAgentThreadStore();
  const { sendMessage } = useAskAgent();

  const agentThread = getAgentThread(thread.id);
  const { messages, isLoading } = agentThread;

  const [selectedArtifactIds, setSelectedArtifactIds] = useState<string[]>([]);
  const [followUpQuestion, setFollowUpQuestion] = useState("");

  const { scrollContainerRef, bottomRef, isAtBottom, scrollToBottom } = useSmartScroll({
    messages
  });

  const isThreadBusy = isLoading || thread.is_processing;
  const shouldShowWarning = messages.length >= MESSAGES_WARNING_THRESHOLD;

  const handleArtifactClick = (id: string) => setSelectedArtifactIds([id]);

  const handleSendMessage = useCallback(() => {
    if (!followUpQuestion.trim() || isLoading) return;

    sendMessage(followUpQuestion, thread.id);
    setFollowUpQuestion("");
    scrollToBottom();
  }, [followUpQuestion, isLoading, scrollToBottom, sendMessage, thread.id]);

  const fetchMessages = useCallback(async () => {
    try {
      const history = await ThreadService.getThreadMessages(projectId, thread.id);
      setMessages(thread.id, history as unknown as Message[]);
    } catch (error) {
      console.error("Failed to fetch message history:", error);
    }
  }, [setMessages, thread.id, projectId]);

  useEffect(() => {
    if ((messages && messages.length > 0) || isLoading) return;

    if (isInitialLoad.current) return;
    isInitialLoad.current = true;
    fetchMessages();
  }, [fetchMessages, isLoading, messages]);

  const handleRefresh = useCallback(() => {
    fetchMessages();
    refetchThread();
  }, [fetchMessages, refetchThread]);

  const onStop = useCallback(async () => {
    try {
      await ThreadService.stopThread(projectId, thread.id);
      refetchThread();
      fetchMessages();
    } catch (error) {
      toast.error(`Failed to stop thread: ${(error as Error).message}`);
      console.error("Failed to stop thread:", error);
    }
  }, [fetchMessages, refetchThread, thread.id, projectId]);

  return (
    <div className='flex h-full flex-col overflow-hidden'>
      <ThreadHeader thread={thread} />

      <div className='flex w-full flex-1 items-center justify-center overflow-hidden'>
        <div className='flex h-full w-full flex-1 flex-col overflow-hidden'>
          <div className='relative w-full flex-1 overflow-hidden'>
            <div ref={scrollContainerRef} className='customScrollbar h-full w-full overflow-auto'>
              <div className='mx-auto w-full max-w-[742px] p-4'>
                <Messages messages={messages} onArtifactClick={handleArtifactClick} />
                <div ref={bottomRef} />
              </div>
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

          <div className='mx-auto flex w-full max-w-[742px] flex-shrink-0 flex-col gap-1 p-4 pt-0'>
            <ProcessingWarning
              threadId={thread.id}
              isLoading={isLoading}
              onRefresh={handleRefresh}
            />

            <MessageInput
              value={followUpQuestion}
              onChange={setFollowUpQuestion}
              onSend={handleSendMessage}
              onStop={onStop}
              disabled={isThreadBusy}
              showWarning={shouldShowWarning}
              isLoading={isThreadBusy}
            />
          </div>
        </div>

        <ArtifactPanelContainer
          messages={messages}
          selectedIds={selectedArtifactIds}
          onSelect={setSelectedArtifactIds}
        />
      </div>
    </div>
  );
};

export default AgentThread;

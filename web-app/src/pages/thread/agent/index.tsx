import { useCallback, useEffect, useRef, useState } from "react";
import dayjs from "dayjs";
import relativeTime from "dayjs/plugin/relativeTime";

import MessageInput from "@/components/MessageInput";
import useAskAgent from "@/hooks/messaging/agent";
import useAgentThreadStore from "@/stores/useAgentThread";
import { Message, ThreadItem } from "@/types/chat";
import Messages from "@/pages/thread/messages";
import ThreadHeader from "./components/ThreadHeader";
import ProcessingWarning from "../ProcessingWarning";
import ArtifactPanelContainer from "./components/ArtifactPanelContainer";
import { ThreadService } from "@/services/api";
import { toast } from "sonner";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

dayjs.extend(relativeTime);

interface AgentThreadProps {
  thread: ThreadItem;
  refetchThread: () => void;
}

const MESSAGES_WARNING_THRESHOLD = 10;

const AgentThread = ({ thread, refetchThread }: AgentThreadProps) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const isInitialLoad = useRef(false);
  const { getAgentThread, setMessages } = useAgentThreadStore();
  const { sendMessage } = useAskAgent();
  const bottomRef = useRef<HTMLDivElement>(null);

  const agentThread = getAgentThread(thread.id);
  const { messages, isLoading } = agentThread;

  const [selectedArtifactIds, setSelectedArtifactIds] = useState<string[]>([]);
  const [followUpQuestion, setFollowUpQuestion] = useState("");

  const shouldShowWarning = messages.length >= MESSAGES_WARNING_THRESHOLD;

  const handleArtifactClick = (id: string) => setSelectedArtifactIds([id]);

  const handleSendMessage = useCallback(() => {
    if (!followUpQuestion.trim() || isLoading) return;

    sendMessage(followUpQuestion, thread.id);
    setFollowUpQuestion("");
  }, [followUpQuestion, isLoading, sendMessage, thread.id]);

  const streamingMessage = messages.find((message) => message.isStreaming);

  useEffect(() => {
    const behavior = streamingMessage?.content ? "instant" : "smooth";
    bottomRef.current?.scrollIntoView({ behavior });
  }, [messages, streamingMessage?.content]);

  const fetchMessages = useCallback(async () => {
    try {
      const history = await ThreadService.getThreadMessages(
        projectId,
        thread.id,
      );
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

  const onStop = useCallback(() => {
    ThreadService.stopThread(projectId, thread.id)
      // eslint-disable-next-line promise/always-return
      .then(() => {
        refetchThread();
        fetchMessages();
      })
      .catch((error) => {
        toast.error(`Failed to stop thread: ${error.message}`);
        console.error("Failed to stop thread:", error);
      });
  }, [fetchMessages, refetchThread, thread.id, projectId]);

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <ThreadHeader thread={thread} />

      <div className="overflow-hidden flex-1 flex items-center w-full justify-center">
        <div className="flex-1 w-full h-full overflow-hidden flex flex-col gap-4">
          <div className="flex-1 w-full customScrollbar overflow-auto">
            <div className="max-w-[742px] w-full p-4 mx-auto">
              <Messages
                messages={messages}
                onArtifactClick={handleArtifactClick}
              />
              <div ref={bottomRef} />
            </div>
          </div>

          <div className="flex flex-col p-4 gap-1 pt-0 max-w-[742px] mx-auto w-full">
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
              onStop={onStop}
              disabled={isLoading || thread.is_processing}
              showWarning={shouldShowWarning}
              isLoading={isLoading || thread.is_processing}
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

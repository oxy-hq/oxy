import { useCallback, useEffect, useRef, useState } from "react";
import dayjs from "dayjs";
import relativeTime from "dayjs/plugin/relativeTime";

import MessageInput from "@/components/MessageInput";
import useAskAgent from "@/hooks/messaging/agent";
import { service } from "@/services/service";
import useAgentThreadStore from "@/stores/useAgentThread";
import { Message, ThreadItem } from "@/types/chat";
import Messages from "@/pages/thread/messages";
import ThreadHeader from "./components/ThreadHeader";
import ProcessingWarning from "../ProcessingWarning";
import ArtifactPanelContainer from "./components/ArtifactPanelContainer";

dayjs.extend(relativeTime);

interface AgentThreadProps {
  thread: ThreadItem;
  refetchThread: () => void;
}

const MESSAGES_WARNING_THRESHOLD = 10;

const AgentThread = ({ thread, refetchThread }: AgentThreadProps) => {
  const isInitialLoad = useRef(false);
  const { getAgentThread, setMessages } = useAgentThreadStore();
  const { sendMessage } = useAskAgent();

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

  const fetchMessages = useCallback(async () => {
    try {
      const history = await service.getThreadMessages(thread.id);
      setMessages(thread.id, history as unknown as Message[]);
    } catch (error) {
      console.error("Failed to fetch message history:", error);
    }
  }, [setMessages, thread.id]);

  useEffect(() => {
    if ((messages && messages.length > 0) || isLoading) return;

    if (isInitialLoad.current) return;
    isInitialLoad.current = true;
    fetchMessages();
  }, [fetchMessages, isLoading, messages]);

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
            </div>
          </div>

          <div className="flex flex-col p-4 gap-1 pt-0 max-w-[742px] mx-auto w-full">
            <ProcessingWarning
              thread={thread}
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
              disabled={isLoading}
              showWarning={shouldShowWarning}
              isLoading={isLoading}
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

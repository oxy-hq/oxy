import MessageInput from "@/components/MessageInput";
import { ThreadItem } from "@/types/chat";
import { useRef, useCallback } from "react";
import { useEffect } from "react";
import { useState } from "react";
import useTaskThreadStore from "@/stores/useTaskThread";
import Header from "./Header";
import ProcessingWarning from "../ProcessingWarning";
import { toast } from "sonner";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import BlockMessage from "@/components/Messages/BlockMessage";
import UserMessage from "@/components/Messages/UserMessage";
import {
  useAgenticStore,
  useAskAgentic,
  useIsThreadLoading,
  useLastRunInfoGroupId,
  useLastStreamingMessage,
  useObserveAgenticMessages,
  useSelectedMessageReasoning,
  useStopAgenticRun,
} from "@/stores/agentic";
import { LoaderCircle } from "lucide-react";
import SidePanel from "./SidePanel";
import { uniqBy } from "lodash";

const AgenticThread = ({ thread }: { thread: ThreadItem }) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const { getTaskThread } = useTaskThreadStore();
  const taskThread = getTaskThread(thread.id);
  const bottomRef = useRef<HTMLDivElement>(null);
  const messages = uniqBy(taskThread.messages, (m) => m.id);
  const { mutateAsync: sendMessage } = useAskAgentic();

  const [followUpQuestion, setFollowUpQuestion] = useState("");
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const shouldShowWarning = messages.length > 10;

  const isLoading = useIsThreadLoading(thread.id);
  const { mutateAsync: stopThread } = useStopAgenticRun(thread.id);
  const { refetch: refetchThreadMessages } = useAgenticStore(
    projectId,
    thread.id,
  );
  useObserveAgenticMessages(thread.id, refetchThreadMessages);
  const { setSelectedGroupId, selectReasoning, selectedGroupId } =
    useSelectedMessageReasoning();
  const streamingContent = useLastStreamingMessage(thread.id);
  const lastRunGroupId = useLastRunInfoGroupId(thread.id);

  useEffect(() => {
    if (lastRunGroupId) {
      setSelectedGroupId(lastRunGroupId);
    }
  }, [lastRunGroupId]);

  useEffect(() => {
    const behavior = streamingContent ? "instant" : "smooth";
    bottomRef.current?.scrollIntoView({ behavior });
  }, [messages, streamingContent]);

  const handleSendMessage = async () => {
    if (!followUpQuestion.trim() || isLoading) return;

    await sendMessage({
      prompt: followUpQuestion,
      threadId: thread.id,
      agentRef: thread.source,
    });
    setFollowUpQuestion("");
  };

  useEffect(() => {
    if (messagesContainerRef.current) {
      messagesContainerRef.current.scrollTop =
        messagesContainerRef.current.scrollHeight;
    }
  }, [messages]);

  const onStop = useCallback(() => {
    stopThread()
      // eslint-disable-next-line promise/always-return
      .then(() => {
        refetchThreadMessages();
      })
      .catch((error) => {
        toast.error(`Failed to stop thread: ${error.message}`);
        console.error("Failed to stop thread:", error);
      });
  }, [refetchThreadMessages, stopThread]);

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
              <div className="mb-6 max-w-page-content mx-auto w-full">
                {messages.length === 0 ? (
                  <div className="flex items-center justify-center h-full">
                    <LoaderCircle className="w-6 h-6 animate-spin text-muted-foreground" />
                  </div>
                ) : (
                  messages.map((msg) => (
                    <div
                      key={msg.id}
                      className={`mb-6 p-4 rounded-lg ${msg.is_human ? "bg-muted/50" : "bg-secondary/20"}`}
                    >
                      {msg.is_human ? (
                        <UserMessage
                          content={msg.content}
                          createdAt={msg.created_at}
                        />
                      ) : (
                        <BlockMessage
                          key={msg.id}
                          message={msg}
                          toggleReasoning={selectReasoning}
                        />
                      )}
                    </div>
                  ))
                )}
              </div>
              <div ref={bottomRef} />
            </div>

            <div className="p-6 pt-0 max-w-page-content mx-auto w-full">
              <ProcessingWarning
                threadId={thread.id}
                isLoading={isLoading}
                onRefresh={() => {
                  refetchThreadMessages();
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

        {!!selectedGroupId && <SidePanel />}
      </div>
    </div>
  );
};

export default AgenticThread;

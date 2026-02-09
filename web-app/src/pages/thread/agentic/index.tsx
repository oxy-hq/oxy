import { uniqBy } from "lodash";
import { LoaderCircle } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import MessageInput from "@/components/MessageInput";
import BlockMessage from "@/components/Messages/BlockMessage";
import UserMessage from "@/components/Messages/UserMessage";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import {
  useAgenticStore,
  useAskAgentic,
  useIsThreadLoading,
  useLastRunInfoGroupId,
  useLastStreamingMessage,
  useObserveAgenticMessages,
  useSelectedMessageReasoning,
  useStopAgenticRun
} from "@/stores/agentic";
import useTaskThreadStore from "@/stores/useTaskThread";
import type { ThreadItem } from "@/types/chat";
import ProcessingWarning from "../ProcessingWarning";
import Header from "./Header";
import SidePanel from "./SidePanel";

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
  const { refetch: refetchThreadMessages } = useAgenticStore(projectId, thread.id);
  useObserveAgenticMessages(thread.id, refetchThreadMessages);
  const { setSelectedGroupId, selectReasoning, selectedGroupId } = useSelectedMessageReasoning();
  const streamingContent = useLastStreamingMessage(thread.id);
  const lastRunGroupId = useLastRunInfoGroupId(thread.id);

  // biome-ignore lint/correctness/useExhaustiveDependencies: <explanation>
  useEffect(() => {
    if (lastRunGroupId) {
      setSelectedGroupId(lastRunGroupId);
    }
  }, [lastRunGroupId]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: <explanation>
  useEffect(() => {
    const behavior = streamingContent ? "instant" : "smooth";
    bottomRef.current?.scrollIntoView({ behavior });
  }, [messages, streamingContent]);

  const handleSendMessage = async () => {
    if (!followUpQuestion.trim() || isLoading) return;

    await sendMessage({
      prompt: followUpQuestion,
      threadId: thread.id,
      agentRef: thread.source
    });
    setFollowUpQuestion("");
  };

  // biome-ignore lint/correctness/useExhaustiveDependencies: <explanation>
  useEffect(() => {
    if (messagesContainerRef.current) {
      messagesContainerRef.current.scrollTop = messagesContainerRef.current.scrollHeight;
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
    <div className='flex h-full flex-col'>
      <Header thread={thread} />
      <div className='flex flex-1 overflow-hidden'>
        <div className='flex h-full flex-1 flex-col'>
          <div className='flex h-full w-full flex-1 flex-col py-4'>
            <div
              ref={messagesContainerRef}
              className='customScrollbar flex w-full flex-1 flex-col overflow-y-auto [scrollbar-gutter:stable_both-edges]'
            >
              <div className='mx-auto mb-6 w-full max-w-page-content'>
                {messages.length === 0 ? (
                  <div className='flex h-full items-center justify-center'>
                    <LoaderCircle className='h-6 w-6 animate-spin text-muted-foreground' />
                  </div>
                ) : (
                  messages.map((msg) => (
                    <div
                      key={msg.id}
                      className={`mb-6 rounded-lg p-4 ${msg.is_human ? "bg-muted/50" : "bg-secondary/20"}`}
                    >
                      {msg.is_human ? (
                        <UserMessage content={msg.content} createdAt={msg.created_at} />
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

            <div className='mx-auto w-full max-w-page-content p-6 pt-0'>
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

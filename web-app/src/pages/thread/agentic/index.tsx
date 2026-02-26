import { uniqBy } from "lodash";
import { LoaderCircle } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import MessageInput from "@/components/MessageInput";
import UserMessage from "@/components/Messages/UserMessage";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

import {
  useAgenticStore,
  useAskAgentic,
  useIsThreadLoading,
  useLastStreamingMessage,
  useObserveAgenticMessages,
  useSelectedMessageReasoning,
  useStopAgenticRun
} from "@/stores/agentic";
import useTaskThreadStore from "@/stores/useTaskThread";
import type { ThreadItem } from "@/types/chat";
import ProcessingWarning from "../ProcessingWarning";
import ArtifactSidebar from "./ArtifactSidebar";
import AutomationDagPanel from "./AutomationDagPanel";
import BlockMessage, { type AutomationGenerated } from "./BlockMessage";
import Header from "./Header";

const AgenticThread = ({ thread }: { thread: ThreadItem }) => {
  const { project } = useCurrentProjectBranch();

  const { getTaskThread } = useTaskThreadStore();
  const taskThread = getTaskThread(thread.id);
  const bottomRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const isUserScrolledUp = useRef(false);

  const messages = useMemo(() => uniqBy(taskThread.messages, (m) => m.id), [taskThread.messages]);
  const shouldShowWarning = messages.length > 10;

  const { mutateAsync: sendMessage } = useAskAgentic();
  const [followUpQuestion, setFollowUpQuestion] = useState("");

  const isLoading = useIsThreadLoading(thread.id);
  const { mutateAsync: stopThread } = useStopAgenticRun(thread.id);
  const { refetch: refetchThreadMessages } = useAgenticStore(project.id, thread.id);
  useObserveAgenticMessages(thread.id, refetchThreadMessages);
  const streamingContent = useLastStreamingMessage(thread.id);

  const [hoveredNodeId, setHoveredNode] = useState<string | null>(null);

  const { selectedBlock, setSelectedBlockId, setSelectedGroupId } = useSelectedMessageReasoning();

  // biome-ignore lint/correctness/useExhaustiveDependencies: only reset on thread change
  useEffect(() => {
    setSelectedBlockId(null);
    setSelectedGroupId(null);
  }, [thread.id]);

  const [automationGenerated, setAutomationGenerated] = useState<AutomationGenerated | undefined>(
    undefined
  );

  useEffect(() => {
    const container = messagesContainerRef.current;
    if (!container) return;
    const handleScroll = () => {
      const distanceFromBottom =
        container.scrollHeight - container.scrollTop - container.clientHeight;
      isUserScrolledUp.current = distanceFromBottom > 100;
    };
    container.addEventListener("scroll", handleScroll);
    return () => container.removeEventListener("scroll", handleScroll);
  }, []);

  // biome-ignore lint/correctness/useExhaustiveDependencies: <explanation>
  useEffect(() => {
    if (isUserScrolledUp.current) return;
    bottomRef.current?.scrollIntoView({
      behavior: streamingContent ? "instant" : "smooth"
    });
  }, [messages, streamingContent]);

  const handleArtifactRerun = useCallback(
    async (prompt: string) => {
      setSelectedBlockId(null);
      isUserScrolledUp.current = false;
      await sendMessage({
        prompt,
        threadId: thread.id,
        agentRef: thread.source
      });
    },
    [sendMessage, thread.id, thread.source, setSelectedBlockId]
  );

  const handleSendMessage = async () => {
    if (!followUpQuestion.trim() || isLoading) return;
    isUserScrolledUp.current = false;
    await sendMessage({
      prompt: followUpQuestion,
      threadId: thread.id,
      agentRef: thread.source
    });
    setFollowUpQuestion("");
  };

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
      <ResizablePanelGroup direction='horizontal' className='flex-1'>
        <ResizablePanel defaultSize={selectedBlock ? 60 : 100} minSize={30}>
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
                  messages.map((msg) =>
                    msg.is_human ? (
                      <div key={msg.id} className='mb-6 flex justify-end'>
                        <UserMessage content={msg.content} createdAt={msg.created_at} />
                      </div>
                    ) : (
                      <div key={msg.id} className='mb-6'>
                        <BlockMessage
                          message={msg}
                          threadId={thread.id}
                          hoveredNodeId={hoveredNodeId}
                          automationGenerated={automationGenerated}
                          onStepHover={setHoveredNode}
                          onAutomationGenerated={setAutomationGenerated}
                        />
                      </div>
                    )
                  )
                )}
              </div>
              <div ref={bottomRef} />
            </div>

            <div className='mx-auto w-full max-w-page-content p-6 pt-0'>
              <ProcessingWarning
                threadId={thread.id}
                isLoading={isLoading}
                onRefresh={refetchThreadMessages}
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
        </ResizablePanel>

        {selectedBlock && (
          <>
            <ResizableHandle />
            <ResizablePanel defaultSize={40} minSize={20} maxSize={70}>
              <ArtifactSidebar
                block={selectedBlock}
                onClose={() => setSelectedBlockId(null)}
                onRerun={handleArtifactRerun}
              />
            </ResizablePanel>
          </>
        )}

        {automationGenerated && (
          <>
            <ResizableHandle />
            <ResizablePanel defaultSize={20} minSize={15} maxSize={40}>
              <AutomationDagPanel
                automationGenerated={automationGenerated}
                highlightedNodeId={hoveredNodeId}
                onNodeHover={setHoveredNode}
                onClose={() => setAutomationGenerated(undefined)}
              />
            </ResizablePanel>
          </>
        )}
      </ResizablePanelGroup>
    </div>
  );
};

export default AgenticThread;

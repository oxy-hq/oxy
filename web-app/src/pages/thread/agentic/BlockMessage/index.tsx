import { useMessageContent, useSelectedMessageReasoning } from "@/stores/agentic";
import type { Message } from "@/types/chat";
import BlockContent from "./BlockContent";
import ReasoningTrace, { type AutomationGenerated } from "./ReasoningTrace";

export { default as BlockContent } from "./BlockContent";
export type { AutomationGenerated } from "./ReasoningTrace";

interface BlockMessageProps {
  message: Message;
  threadId: string;
  hoveredNodeId: string | null;
  automationGenerated?: AutomationGenerated;
  onStepHover: (id: string | null) => void;
  onAutomationGenerated?: (automation: AutomationGenerated) => void;
}

const BlockMessage = ({
  message,
  threadId,
  hoveredNodeId,
  automationGenerated,
  onStepHover,
  onAutomationGenerated
}: BlockMessageProps) => {
  const { run_info: runInfo } = message;
  const { selectBlock } = useSelectedMessageReasoning();
  const content = useMessageContent(runInfo);

  const error = runInfo?.error || (runInfo?.status === "canceled" && "Agent run was cancelled");

  if (!runInfo) {
    return null;
  }

  return (
    <div className='flex w-full flex-col gap-3'>
      <ReasoningTrace
        runInfo={runInfo}
        threadId={threadId}
        automationGenerated={automationGenerated}
        hoveredNodeId={hoveredNodeId}
        onAutomationGenerated={onAutomationGenerated}
        onStepHover={onStepHover}
      />

      {error ? (
        <span className='text-red-800'>{error}</span>
      ) : (
        content?.map((block) => (
          <BlockContent
            key={block.id}
            block={block}
            onFullscreen={(blockId) => selectBlock(blockId, runInfo)}
          />
        ))
      )}
    </div>
  );
};

export default BlockMessage;

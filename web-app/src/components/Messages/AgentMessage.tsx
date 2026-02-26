import AnswerContent from "@/components/AnswerContent";
import ThreadReferences from "@/components/ThreadReferences";
import ThreadSteps from "@/components/ThreadSteps";
import useTheme from "@/stores/useTheme";
import type { Message } from "@/types/chat";
import MessageHeader from "./MessageHeader";

interface AgentMessageProps {
  message: Message;
  showAvatar?: boolean;
  prompt?: string;
  onArtifactClick?: (id: string) => void;
}

const AgentMessage = ({ message, showAvatar, prompt, onArtifactClick }: AgentMessageProps) => {
  const { content, references, steps, isStreaming } = message;
  const showAnswer = content || steps?.length > 0 || !isStreaming;
  const showAgentThinking = isStreaming && !showAnswer;
  const { theme } = useTheme();

  return (
    <div className='mb-4 flex w-full flex-col'>
      <MessageHeader
        isHuman={false}
        createdAt={message.created_at}
        tokensUsage={{
          inputTokens: message.usage.inputTokens,
          outputTokens: message.usage.outputTokens
        }}
      />
      {showAgentThinking && (
        <div className='mt-2 flex items-start gap-2' data-testid='agent-loading-state'>
          <img
            className='h-8 w-8'
            src={theme === "dark" ? "/oxy-loading-dark.gif" : "/oxy-loading.gif"}
          />
          <div className='rounded-xl bg-muted px-4 py-2'>
            <p className='text-muted-foreground'>Agent is thinking...</p>
          </div>
        </div>
      )}
      {showAnswer && (
        <div className='flex w-full items-start' data-testid='agent-message-container'>
          {showAvatar && <img className='h-8 w-8 rounded-full' src='/logo.svg' alt='Oxy' />}
          <div className='w-full flex-1'>
            <div
              className='flex w-full flex-col gap-2 overflow-x-auto shadow-sm'
              data-testid='agent-message-content'
            >
              <ThreadSteps steps={steps} isLoading={isStreaming} />
              <AnswerContent content={content} onArtifactClick={onArtifactClick} />
            </div>
            {references?.length > 0 && (
              <div className='mt-2'>
                <ThreadReferences references={references} prompt={prompt} />
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
};

export default AgentMessage;

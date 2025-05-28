import AnswerContent from "@/components/AnswerContent";
import ThreadSteps from "@/components/ThreadSteps";
import ThreadReferences from "@/components/ThreadReferences";
import { Message } from "@/types/chat";
import useTheme from "@/stores/useTheme";
import MessageHeader from "./MessageHeader";

interface AgentMessageProps {
  message: Message;
  showAvatar?: boolean;
  prompt?: string;
  createdAt?: string;
}

const AgentMessage = ({
  message,
  showAvatar,
  prompt,
  createdAt,
}: AgentMessageProps) => {
  const { content, references, steps, isStreaming } = message;
  const showAnswer = content || steps.length > 0;
  const showAgentThinking = isStreaming && !showAnswer;
  const { theme } = useTheme();

  return (
    <div className="flex flex-col gap-2">
      <MessageHeader isHuman={false} createdAt={createdAt} />
      {showAgentThinking && (
        <div className="flex gap-2 items-start">
          <img
            className="w-8 h-8"
            src={
              theme === "dark" ? "/oxy-loading-dark.gif" : "/oxy-loading.gif"
            }
          />
          <div className="bg-muted px-4 py-2 rounded-xl">
            <p className="text-muted-foreground">Agent is thinking...</p>
          </div>
        </div>
      )}
      {showAnswer && (
        <div className="flex gap-2 items-start">
          {showAvatar && (
            <img className="w-8 h-8 rounded-full" src="/logo.svg" alt="Oxy" />
          )}
          <div className="flex-1">
            <div className="p-6 rounded-xl bg-base-card border border-base-border shadow-sm flex flex-col gap-2">
              <ThreadSteps steps={steps} isLoading={isStreaming} />
              <AnswerContent content={content || ""} />
            </div>
            {references.length > 0 && (
              <div className="mt-2">
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

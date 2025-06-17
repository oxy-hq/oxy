import AgentMessage from "@/components/Messages/AgentMessage";
import { Message } from "@/types/chat";

interface StreamingMessageProps {
  message: Message;
  onArtifactClick?: (id: string) => void;
}

const StreamingMessage = ({
  message,
  onArtifactClick,
}: StreamingMessageProps) => {
  if (!message.isStreaming) return null;

  return (
    <div className="mb-6 p-4 rounded-lg bg-secondary/20">
      <AgentMessage message={message} onArtifactClick={onArtifactClick} />
    </div>
  );
};

export default StreamingMessage;

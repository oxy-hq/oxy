import AgentMessage from "@/components/Messages/AgentMessage";
import { Message } from "@/types/chat";

interface StreamingMessageProps {
  message: Message;
}

const StreamingMessage = ({ message }: StreamingMessageProps) => {
  if (!message.isStreaming) return null;

  return (
    <div className="mb-6 p-4 rounded-lg bg-secondary/20">
      <AgentMessage message={message} />
    </div>
  );
};

export default StreamingMessage;

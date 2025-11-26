import { memo } from "react";
import AgentMessage from "@/components/Messages/AgentMessage";
import UserMessage from "@/components/Messages/UserMessage";
import { Message } from "@/types/chat";

interface Props {
  msg: Message;
  onArtifactClick?: (id: string) => void;
}

const MessageItem = memo(({ msg, onArtifactClick }: Props) => (
  <div
    key={msg.id}
    className={`mb-6 p-4 rounded-lg ${msg.is_human ? "bg-muted/50" : "bg-secondary/20"}`}
    data-testid={`message-${msg.is_human ? 'human' : 'agent'}`}
  >
    {msg.is_human ? (
      <UserMessage content={msg.content} createdAt={msg.created_at} />
    ) : (
      <AgentMessage message={msg} onArtifactClick={onArtifactClick} />
    )}
  </div>
));

export default MessageItem;

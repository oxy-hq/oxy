import AgentMessage from "@/components/Messages/AgentMessage";
import UserMessage from "@/components/Messages/UserMessage";
import { MessageItem } from "@/types/chat";

interface MessageHistoryItemProps {
  msg: MessageItem;
  onArtifactClick?: (id: string) => void;
}

const MessageHistoryItem = ({
  msg,
  onArtifactClick,
}: MessageHistoryItemProps) => (
  <div
    key={msg.id}
    className={`mb-6 p-4 rounded-lg ${msg.is_human ? "bg-muted/50" : "bg-secondary/20"}`}
  >
    {msg.is_human ? (
      <UserMessage content={msg.content} createdAt={msg.created_at} />
    ) : (
      <AgentMessage
        message={{
          content: msg.content,
          isUser: false,
          references: [],
          steps: [],
          isStreaming: false,
          usage: msg.usage,
        }}
        createdAt={msg.created_at}
        onArtifactClick={onArtifactClick}
      />
    )}
  </div>
);

export default MessageHistoryItem;

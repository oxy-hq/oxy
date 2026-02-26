import { memo } from "react";
import AgentMessage from "@/components/Messages/AgentMessage";
import UserMessage from "@/components/Messages/UserMessage";
import type { Message } from "@/types/chat";

interface Props {
  msg: Message;
  onArtifactClick?: (id: string) => void;
}

const MessageItem = memo(({ msg, onArtifactClick }: Props) => (
  <div>
    {msg.is_human ? (
      <div className='mb-6 flex justify-end'>
        <UserMessage content={msg.content} createdAt={msg.created_at} />
      </div>
    ) : (
      <AgentMessage message={msg} onArtifactClick={onArtifactClick} />
    )}
  </div>
));

export default MessageItem;

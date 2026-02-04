import AgentMessage from "@/components/Messages/AgentMessage";
import type { Message } from "@/types/chat";
import UserMessage from "./UserMessage";

const Messages = ({
  messages,
  onArtifactClick
}: {
  messages: Message[];
  onArtifactClick: (id: string) => void;
}) => {
  return (
    <>
      {messages.map((message, index) => (
        <div key={index}>
          {message.is_human ? (
            <UserMessage message={message} />
          ) : (
            <AgentMessage showAvatar message={message} onArtifactClick={onArtifactClick} />
          )}
        </div>
      ))}
    </>
  );
};

export default Messages;

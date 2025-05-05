import AgentMessage from "@/components/AgentMessage";
import UserMessage from "./UserMessage";
import { Message } from "@/types/chat";

const Messages = ({ messages }: { messages: Message[] }) => {
  return (
    <>
      {messages.map((message, index) => (
        <div key={index}>
          {message.isUser ? (
            <UserMessage message={message} />
          ) : (
            <AgentMessage showAvatar message={message} />
          )}
        </div>
      ))}
    </>
  );
};

export default Messages;

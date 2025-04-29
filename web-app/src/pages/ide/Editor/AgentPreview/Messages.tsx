import AgentMessage from "./AgentMessage";
import UserMessage from "./UserMessage";
import { Message } from ".";

const Messages = ({ messages }: { messages: Message[] }) => {
  return (
    <>
      {messages.map((message, index) => (
        <div key={index}>
          {message.isUser ? (
            <UserMessage message={message} />
          ) : (
            <AgentMessage message={message} />
          )}
        </div>
      ))}
    </>
  );
};

export default Messages;

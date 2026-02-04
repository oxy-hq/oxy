import type React from "react";
import MessageHeader from "./MessageHeader";

interface UserMessageProps {
  content: string;
  createdAt?: string;
}

const UserMessage: React.FC<UserMessageProps> = ({ content, createdAt }) => {
  return (
    <div data-testid='user-message-container'>
      <MessageHeader isHuman={true} createdAt={createdAt} />
      <div
        className='flex flex-col gap-2 rounded-xl border border-base-border bg-base-card p-4 shadow-sm'
        data-testid='user-message-text'
      >
        {content}
      </div>
    </div>
  );
};

export default UserMessage;

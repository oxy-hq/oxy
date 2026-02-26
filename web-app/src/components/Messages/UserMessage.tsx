import type React from "react";

interface UserMessageProps {
  content: string;
  createdAt?: string;
}

const UserMessage: React.FC<UserMessageProps> = ({ content }) => {
  return (
    <div
      className='inline-block max-w-[80%] rounded-2xl bg-secondary px-4 py-2.5 text-foreground text-sm'
      data-testid='user-message-text'
    >
      {content}
    </div>
  );
};

export default UserMessage;

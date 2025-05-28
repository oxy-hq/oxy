import React from "react";
import MessageHeader from "./MessageHeader";

interface UserMessageProps {
  content: string;
  createdAt?: string;
}

const UserMessage: React.FC<UserMessageProps> = ({ content, createdAt }) => {
  return (
    <div>
      <MessageHeader isHuman={true} createdAt={createdAt} />
      <div className="p-6 rounded-xl bg-base-card border border-base-border shadow-sm flex flex-col gap-2">
        {content}
      </div>
    </div>
  );
};

export default UserMessage;

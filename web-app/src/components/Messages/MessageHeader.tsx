import { Bot, User } from "lucide-react";

import MessageInfo from "./MessageInfo";

type TokensUsage = {
  inputTokens: number;
  outputTokens: number;
};

interface MessageHeaderProps {
  isHuman: boolean;
  createdAt?: string;
  tokensUsage?: TokensUsage;
}

const MessageHeader = ({
  isHuman,
  createdAt,
  tokensUsage,
}: MessageHeaderProps) => {
  return (
    <div className="flex items-center gap-2 mb-2">
      {isHuman ? <User className="w-4 h-4" /> : <Bot className="w-4 h-4" />}
      <span className="text-sm font-medium">{isHuman ? "You" : "Agent"}</span>
      <MessageInfo createdAt={createdAt} tokensUsage={tokensUsage} />
    </div>
  );
};

export default MessageHeader;

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

const MessageHeader = ({ isHuman, createdAt, tokensUsage }: MessageHeaderProps) => {
  return (
    <div className='mb-2 flex items-center gap-2'>
      {isHuman ? <User className='h-4 w-4' /> : <Bot className='h-4 w-4' />}
      <span className='font-medium text-sm'>{isHuman ? "You" : "Agent"}</span>
      <MessageInfo createdAt={createdAt} tokensUsage={tokensUsage} />
    </div>
  );
};

export default MessageHeader;

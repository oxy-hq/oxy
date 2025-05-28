import { Bot, User } from "lucide-react";
import dayjs from "dayjs";

interface MessageHeaderProps {
  isHuman: boolean;
  createdAt?: string;
  showTimestamp?: boolean;
}

const MessageHeader = ({
  isHuman,
  createdAt,
  showTimestamp = true,
}: MessageHeaderProps) => (
  <div className="flex items-center gap-2 mb-2">
    {isHuman ? <User className="w-4 h-4" /> : <Bot className="w-4 h-4 " />}
    <span className="text-sm font-medium">{isHuman ? "You" : "Agent"}</span>
    {showTimestamp && createdAt && (
      <span className="text-xs text-muted-foreground ml-auto">
        {dayjs(createdAt).fromNow()}
      </span>
    )}
  </div>
);

export default MessageHeader;

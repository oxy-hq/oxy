import { MessageItem } from "@/types/chat";
import { LoaderCircle } from "lucide-react";
import MessageHistoryItem from "./MessageHistoryItem";

interface MessageHistoryProps {
  messages: MessageItem[];
}

const MessageHistory = ({ messages }: MessageHistoryProps) => {
  if (messages.length === 0) {
    return (
      <div className="flex items-center justify-center h-full">
        <LoaderCircle className="w-6 h-6 animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <div className="mb-6">
      {messages.map((msg) => (
        <MessageHistoryItem key={msg.id} msg={msg} />
      ))}
    </div>
  );
};

export default MessageHistory;

import { Message } from "@/types/chat";
import { LoaderCircle } from "lucide-react";
import MessageItem from "./Item";

interface Props {
  messages: Message[];
  onArtifactClick?: (id: string) => void;
}

const Messages = ({ messages, onArtifactClick }: Props) => {
  return (
    <div className="mb-6 max-w-page-content mx-auto w-full">
      {messages.length === 0 ? (
        <div className="flex items-center justify-center h-full">
          <LoaderCircle className="w-6 h-6 animate-spin text-muted-foreground" />
        </div>
      ) : (
        messages.map((msg) => (
          <MessageItem
            key={msg.id}
            msg={msg}
            onArtifactClick={onArtifactClick}
          />
        ))
      )}
    </div>
  );
};

export default Messages;

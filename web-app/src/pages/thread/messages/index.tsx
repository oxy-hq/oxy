import { LoaderCircle } from "lucide-react";
import type { Message } from "@/types/chat";
import MessageItem from "./Item";

interface Props {
  messages: Message[];
  onArtifactClick?: (id: string) => void;
}

const Messages = ({ messages, onArtifactClick }: Props) => {
  return (
    <div className='mx-auto mb-6 w-full max-w-page-content px-2'>
      {messages.length === 0 ? (
        <div className='flex h-full items-center justify-center'>
          <LoaderCircle className='h-6 w-6 animate-spin text-muted-foreground' />
        </div>
      ) : (
        messages.map((msg) => (
          <MessageItem key={msg.id} msg={msg} onArtifactClick={onArtifactClick} />
        ))
      )}
    </div>
  );
};

export default Messages;

import { Message } from "@/types/chat";

export interface MessageHandlers {
  onMessageUpdate: (message: Message) => void;
  onUserMessage?: (message: Message) => void;
  onFilePathUpdate?: (filePath: string) => void;
}

export interface SendMessageOptions {
  content: string | null;
  threadId: string;
  projectId: string;
  branchName: string;
  metadata?: {
    isPreview?: boolean;
    agentPathb64?: string;
  };
}

export interface MessageSender {
  sendMessage(
    options: SendMessageOptions,
    handlers: MessageHandlers,
  ): Promise<void>;
}

export interface ThreadStore {
  getThread: (threadId: string) => { messages: Message[]; isLoading: boolean };
  setIsLoading: (threadId: string, isLoading: boolean) => void;
  setMessages: (threadId: string, messages: Message[]) => void;
  setFilePath?: (threadId: string, filePath: string | undefined) => void;
}

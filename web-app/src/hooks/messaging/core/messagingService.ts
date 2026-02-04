import type { useQueryClient } from "@tanstack/react-query";
import type { Message, ThreadsResponse } from "@/types/chat";
import queryKeys from "../../api/queryKey";
import { MessageFactory } from "./messageFactory";
import type { MessageHandlers, MessageSender, SendMessageOptions, ThreadStore } from "./types";

export const ERROR_MESSAGES = {
  PROCESSING_ERROR: "Error occurred while processing your request."
} as const;

export class MessagingService {
  private messageSender;
  private threadStore;
  private queryClient;

  constructor(
    messageSender: MessageSender,
    threadStore: ThreadStore,
    queryClient: ReturnType<typeof useQueryClient>
  ) {
    this.messageSender = messageSender;
    this.threadStore = threadStore;
    this.queryClient = queryClient;
  }

  async sendMessage(options: SendMessageOptions): Promise<void> {
    const { threadId, projectId } = options;
    const { messages, isLoading } = this.threadStore.getThread(threadId);

    if (isLoading) return;

    this.queryClient.setQueryData(
      queryKeys.thread.list(projectId, 1, 50),
      (old: ThreadsResponse | undefined) => {
        if (old) {
          return {
            ...old,
            threads: old.threads.map((item) =>
              item.id === threadId ? { ...item, is_processing: true } : item
            )
          };
        }
        return old;
      }
    );

    this.threadStore.setIsLoading(threadId, true);
    const newMessages: Message[] = [...messages];
    let currentStreamingMessage: Message | null = null;

    const handlers: MessageHandlers = {
      onMessageUpdate: (message: Message) => {
        currentStreamingMessage = message;
        this.updateMessageInArray(newMessages, message);
        this.threadStore.setMessages(threadId, newMessages);
      },
      onUserMessage: (userMessage: Message) => {
        newMessages.push(userMessage);
        this.threadStore.setMessages(threadId, newMessages);
      },
      onFilePathUpdate: this.threadStore.setFilePath
        ? (filePath: string) => this.threadStore.setFilePath?.(threadId, filePath)
        : undefined
    };

    try {
      await this.messageSender.sendMessage(options, handlers);
    } catch (error) {
      console.error("Error sending message:", error);
      this.handleError(currentStreamingMessage, threadId, handlers.onMessageUpdate);
    } finally {
      this.finalizeSending(currentStreamingMessage, threadId, handlers.onMessageUpdate);
    }
  }

  private updateMessageInArray(messages: Message[], newMessage: Message): void {
    const messageIndex = messages.findIndex((msg) => msg.id === newMessage.id);
    if (messageIndex >= 0) {
      messages[messageIndex] = newMessage;
    } else {
      messages.push(newMessage);
    }
  }

  private handleError(
    currentStreamingMessage: Message | null,
    threadId: string,
    onMessageUpdate: (message: Message) => void
  ): void {
    const errorMessage = MessageFactory.createErrorMessage(
      threadId,
      ERROR_MESSAGES.PROCESSING_ERROR,
      currentStreamingMessage?.id
    );
    onMessageUpdate(errorMessage);
  }

  private finalizeSending(
    currentStreamingMessage: Message | null,
    threadId: string,
    onMessageUpdate: (message: Message) => void
  ): void {
    if (currentStreamingMessage?.isStreaming) {
      const completedMessage = MessageFactory.completeStreamingMessage(currentStreamingMessage);
      onMessageUpdate(completedMessage);
    }

    this.queryClient.invalidateQueries({
      queryKey: queryKeys.thread.all
    });

    this.threadStore.setIsLoading(threadId, false);
  }
}

import { ThreadService } from "@/services/api";
import { Chunk } from "@/types/app";
import { Answer } from "@/types/chat";
import {
  MessageSender,
  SendMessageOptions,
  MessageHandlers,
} from "../core/types";
import { MessageFactory } from "../core/messageFactory";
import { TaskMessageProcessor } from "./processors/processor";

export class TaskMessageSender implements MessageSender {
  private processor = new TaskMessageProcessor();

  async sendMessage(
    options: SendMessageOptions,
    handlers: MessageHandlers,
  ): Promise<void> {
    const { content, threadId } = options;
    const { onMessageUpdate, onUserMessage, onFilePathUpdate } = handlers;

    let streamingMessage = MessageFactory.createStreamingMessage(threadId);

    await ThreadService.askTask(
      threadId,
      content,
      (answer: Answer) => {
        streamingMessage = this.processor.processContent(
          streamingMessage,
          answer as unknown as Chunk,
        );
        onMessageUpdate(streamingMessage);

        if ((answer as unknown as Chunk).file_path && onFilePathUpdate) {
          onFilePathUpdate((answer as unknown as Chunk).file_path);
        }
      },
      () => {
        if (content && onUserMessage) {
          const userMessage = MessageFactory.createUserMessage(
            content,
            threadId,
          );
          onUserMessage(userMessage);
          onMessageUpdate(streamingMessage);
        }
      },
    );
  }
}

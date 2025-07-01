import { service } from "@/services/service";
import { Chunk } from "@/types/app";
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

    await service.askTask(
      threadId,
      content,
      (chunk: Chunk) => {
        streamingMessage = this.processor.processContent(
          streamingMessage,
          chunk,
        );
        onMessageUpdate(streamingMessage);

        if (chunk.file_path && onFilePathUpdate) {
          onFilePathUpdate(chunk.file_path);
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

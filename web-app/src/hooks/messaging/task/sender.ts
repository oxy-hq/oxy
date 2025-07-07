import { ThreadService } from "@/services/api";
import { Answer } from "@/types/chat";
import {
  MessageSender,
  SendMessageOptions,
  MessageHandlers,
} from "../core/types";
import { MessageFactory } from "../core/messageFactory";
import { MessageProcessor } from "../core/processors/processor";

export class TaskMessageSender implements MessageSender {
  private processor = new MessageProcessor();

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
          answer,
        );
        onMessageUpdate(streamingMessage);

        if (streamingMessage.file_path && onFilePathUpdate) {
          onFilePathUpdate(streamingMessage.file_path);
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

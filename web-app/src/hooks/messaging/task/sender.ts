import { ThreadService } from "@/services/api";
import type { Answer } from "@/types/chat";
import { MessageFactory } from "../core/messageFactory";
import { MessageProcessor } from "../core/processors/processor";
import type { MessageHandlers, MessageSender, SendMessageOptions } from "../core/types";

export class TaskMessageSender implements MessageSender {
  private processor = new MessageProcessor();

  async sendMessage(options: SendMessageOptions, handlers: MessageHandlers): Promise<void> {
    const { content, threadId, projectId } = options;
    const { onMessageUpdate, onUserMessage, onFilePathUpdate } = handlers;

    let streamingMessage = MessageFactory.createStreamingMessage(threadId);

    if (content && onUserMessage) {
      const userMessage = MessageFactory.createUserMessage(content, threadId);
      onUserMessage(userMessage);
      onMessageUpdate(streamingMessage);
    }

    await ThreadService.askTask(
      projectId,
      threadId,
      content,
      (answer: Answer) => {
        streamingMessage = this.processor.processContent(streamingMessage, answer);
        onMessageUpdate(streamingMessage);

        if (streamingMessage.file_path && onFilePathUpdate) {
          onFilePathUpdate(streamingMessage.file_path);
        }
      },
      () => {}
    );
  }
}

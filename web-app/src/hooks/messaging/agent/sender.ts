import { ThreadService } from "@/services/api";
import { Answer } from "@/types/chat";
import {
  MessageSender,
  SendMessageOptions,
  MessageHandlers,
} from "../core/types";
import { MessageFactory } from "../core/messageFactory";
import { AgentMessageProcessor } from "./processors/processor";

export class AgentMessageSender implements MessageSender {
  private processor = new AgentMessageProcessor();

  async sendMessage(
    options: SendMessageOptions,
    handlers: MessageHandlers,
  ): Promise<void> {
    const { content, threadId, isPreview } = options;

    if (isPreview) {
      await this.sendPreviewMessage(content, threadId, handlers);
    } else {
      await this.sendRegularMessage(content, threadId, handlers);
    }
  }

  private async sendPreviewMessage(
    content: string | null,
    threadId: string,
    handlers: MessageHandlers,
  ): Promise<void> {
    const { onMessageUpdate, onUserMessage } = handlers;

    let streamingMessage = MessageFactory.createStreamingMessage(threadId);

    if (content && onUserMessage) {
      const userMessage = MessageFactory.createUserMessage(content, threadId);
      onUserMessage(userMessage);
    }

    onMessageUpdate(streamingMessage);

    await ThreadService.ask(threadId, content ?? "", (answer: Answer) => {
      streamingMessage = this.processor.processContent(
        streamingMessage,
        answer,
      );
      onMessageUpdate(streamingMessage);
    });
  }

  private async sendRegularMessage(
    content: string | null,
    threadId: string,
    handlers: MessageHandlers,
  ): Promise<void> {
    const { onMessageUpdate, onUserMessage } = handlers;

    let streamingMessage = MessageFactory.createStreamingMessage(threadId);

    await ThreadService.ask(
      threadId,
      content,
      (answer: Answer) => {
        streamingMessage = this.processor.processContent(
          streamingMessage,
          answer,
        );
        onMessageUpdate(streamingMessage);
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

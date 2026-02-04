import { AgentService, ThreadService } from "@/services/api";
import type { Answer } from "@/types/chat";
import { MessageFactory } from "../core/messageFactory";
import { MessageProcessor } from "../core/processors/processor";
import type { MessageHandlers, MessageSender, SendMessageOptions } from "../core/types";

export class AgentMessageSender implements MessageSender {
  private processor = new MessageProcessor();

  async sendMessage(options: SendMessageOptions, handlers: MessageHandlers): Promise<void> {
    const { content, threadId, projectId, branchName, metadata } = options;

    if (metadata?.isPreview) {
      await this.sendPreviewMessage(
        content,
        threadId,
        metadata?.agentPathb64,
        projectId,
        branchName,
        handlers
      );
    } else {
      await this.sendRegularMessage(content, threadId, projectId, handlers);
    }
  }

  private async sendPreviewMessage(
    content: string | null,
    threadId: string,
    agentPathb64: string | undefined,
    projectId: string,
    branchName: string,
    handlers: MessageHandlers
  ): Promise<void> {
    const { onMessageUpdate, onUserMessage } = handlers;

    let streamingMessage = MessageFactory.createStreamingMessage(threadId);

    if (content && onUserMessage) {
      const userMessage = MessageFactory.createUserMessage(content, threadId);
      onUserMessage(userMessage);
    }

    onMessageUpdate(streamingMessage);

    await AgentService.askAgentPreview(
      projectId,
      branchName,
      agentPathb64 ?? "",
      content ?? "",
      (answer: Answer) => {
        streamingMessage = this.processor.processContent(streamingMessage, answer);
        onMessageUpdate(streamingMessage);
      }
    );
  }

  private async sendRegularMessage(
    content: string | null,
    threadId: string,
    projectId: string,
    handlers: MessageHandlers
  ): Promise<void> {
    const { onMessageUpdate, onUserMessage } = handlers;

    let streamingMessage = MessageFactory.createStreamingMessage(threadId);

    if (content && onUserMessage) {
      const userMessage = MessageFactory.createUserMessage(content, threadId);
      onUserMessage(userMessage);
      onMessageUpdate(streamingMessage);
    }

    await ThreadService.askAgent(
      projectId,
      threadId,
      content,
      (answer: Answer) => {
        streamingMessage = this.processor.processContent(streamingMessage, answer);
        onMessageUpdate(streamingMessage);
      },
      () => {}
    );
  }
}

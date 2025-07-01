import { Message } from "@/types/chat";
import { Chunk } from "@/types/app";
import { MessageProcessor } from "../../core/types";

export class TaskMessageProcessor implements MessageProcessor {
  processContent(streamingMessage: Message, chunk: Chunk): Message {
    return {
      ...streamingMessage,
      content: streamingMessage.content + chunk.content,
      isStreaming: true,
    };
  }
}

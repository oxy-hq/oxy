import { service } from "@/services/service";
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import queryKeys from "./queryKey";
import { Message, MessageItem } from "@/types/chat";
import { STEP_MAP } from "@/types/agent";

interface UseSendMessageMutationProps {
  threadId: string;
  onStreamingMessage?: (message: Message) => void;
  onMessagesUpdated?: (messages: MessageItem[]) => void;
  onMessageSent?: (message: MessageItem) => void;
}

const useSendMessageMutation = ({
  threadId,
  onMessageSent,
  onStreamingMessage,
  onMessagesUpdated,
}: UseSendMessageMutationProps) => {
  const queryClient = useQueryClient();
  const [isLoading, setIsLoading] = useState(false);
  const [streamingMessage, setStreamingMessage] = useState<Message>({
    content: "",
    references: [],
    steps: [],
    isUser: false,
    isStreaming: false,
  });

  const sendMessage = async (content: string | null) => {
    if (isLoading) return;

    setIsLoading(true);

    // Initialize streaming message
    const initialStreamingMessage: Message = {
      content: "",
      references: [],
      steps: [],
      isUser: false,
      isStreaming: true,
    };
    setStreamingMessage(initialStreamingMessage);
    onStreamingMessage?.(initialStreamingMessage);

    try {
      await service.ask(
        threadId,
        content,
        (answer) => {
          setStreamingMessage((prevMessage) => {
            const { content: prevContent, references, steps } = prevMessage;
            const shouldAddStep =
              answer.step &&
              Object.keys(STEP_MAP).includes(answer.step) &&
              steps.at(-1) !== answer.step;

            const updatedMessage = {
              content: prevContent + answer.content,
              references: answer.references
                ? [...references, ...answer.references]
                : references,
              steps: shouldAddStep ? [...steps, answer.step] : steps,
              isUser: false,
              isStreaming: true,
            };

            onStreamingMessage?.(updatedMessage);
            return updatedMessage;
          });
        },
        () => {
          if (content) {
            const userMessage: MessageItem = {
              id: `temp-${Date.now()}`,
              content,
              is_human: true,
              created_at: new Date().toISOString(),
              thread_id: threadId,
            };
            onMessageSent?.(userMessage);
          }
        },
      );
    } catch (error) {
      console.error("Error asking question:", error);
      const errorMessage = {
        ...streamingMessage,
        content:
          streamingMessage.content +
          "\n\nError occurred while processing your request.",
        isStreaming: false,
      };
      setStreamingMessage(errorMessage);
      onStreamingMessage?.(errorMessage);
    } finally {
      setIsLoading(false);
      setStreamingMessage((prev) => ({ ...prev, isStreaming: false }));

      queryClient.invalidateQueries({
        queryKey: queryKeys.thread.all,
      });

      // Refresh message history
      try {
        const updatedMessages = await service.getThreadMessages(threadId);
        onMessagesUpdated?.(updatedMessages);

        // Reset streaming message
        const resetMessage = {
          content: "",
          references: [],
          steps: [],
          isUser: false,
          isStreaming: false,
        };
        setStreamingMessage(resetMessage);
        onStreamingMessage?.(resetMessage);
      } catch (error) {
        console.error("Failed to refresh message history:", error);
      }
    }
  };

  return {
    sendMessage,
    isLoading,
    streamingMessage,
  };
};

export default useSendMessageMutation;

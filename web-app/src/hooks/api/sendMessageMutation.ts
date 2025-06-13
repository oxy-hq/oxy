import { service } from "@/services/service";
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import queryKeys from "./queryKey";
import {
  ArtifactDoneContent,
  ArtifactStartedContent,
  ArtifactValueContent,
  Message,
  MessageItem,
  TextContent,
} from "@/types/chat";
import { STEP_MAP } from "@/types/agent";
import { AgentArtifact, Artifact, WorkflowArtifact } from "@/services/mock";
import { useRef } from "react";

interface UseSendMessageMutationProps {
  threadId: string;
  onStreamingMessage?: (message: Message) => void;
  onMessagesUpdated?: (messages: MessageItem[]) => void;
  onMessageSent?: (message: MessageItem) => void;
  onStreamingArtifact?: (artifacts: { [key: string]: Artifact }) => void;
}

const extractUpdatedValue = (
  updatedArtifact: Artifact,
  artifact_value: ArtifactValueContent,
) => {
  let updatedValue = {};
  switch (artifact_value.value.type) {
    case "log_item": {
      const output =
        (updatedArtifact as WorkflowArtifact).content.value.output ?? [];
      const lastItem = output[output.length - 1];

      if (artifact_value.value.value.append && lastItem.append) {
        output[output.length - 1] = {
          ...lastItem,
          content: `${lastItem.content}${artifact_value.value.value.content}`,
        };
      } else {
        output.push(artifact_value.value.value);
      }
      updatedValue = {
        output: [...output],
      };
      break;
    }
    case "content": {
      updatedValue = {
        output: `${(updatedArtifact as AgentArtifact).content.value.output ?? ""}${artifact_value.value.value}`,
      };
      break;
    }
    case "execute_sql": {
      updatedValue = artifact_value.value.value;
      break;
    }
    default: {
      break;
    }
  }
  return updatedValue;
};

const useSendMessageMutation = ({
  threadId,
  onMessageSent,
  onStreamingMessage,
  onMessagesUpdated,
  onStreamingArtifact,
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
  const artifactStreamingDataRef = useRef<{ [key: string]: Artifact }>({});

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
          switch (answer.content.type) {
            case "text": {
              const answer_content = answer.content as TextContent;
              setStreamingMessage((prevMessage) => {
                const { content: prevContent, references, steps } = prevMessage;
                const shouldAddStep =
                  answer.step &&
                  Object.keys(STEP_MAP).includes(answer.step) &&
                  steps.at(-1) !== answer.step;

                const updatedMessage = {
                  content: prevContent + answer_content.content,
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
              break;
            }
            case "artifact_started": {
              const artifact_started = answer.content as ArtifactStartedContent;
              artifactStreamingDataRef.current = {
                ...artifactStreamingDataRef.current,
                [artifact_started.id]: {
                  id: artifact_started.id,
                  name: artifact_started.title,
                  kind: artifact_started.kind.type,
                  is_streaming: true,
                  content: {
                    type: artifact_started.kind.type,
                    value: artifact_started.kind.value,
                  },
                } as Artifact,
              };
              onStreamingArtifact?.(artifactStreamingDataRef.current);
              break;
            }
            case "artifact_done": {
              const artifact_done = answer.content as ArtifactDoneContent;
              artifactStreamingDataRef.current = {
                ...artifactStreamingDataRef.current,
                [artifact_done.id]: {
                  ...artifactStreamingDataRef.current[artifact_done.id],
                  is_streaming: false,
                } as Artifact,
              };
              onStreamingArtifact?.(artifactStreamingDataRef.current);
              break;
            }
            case "artifact_value": {
              const artifact_value = answer.content as ArtifactValueContent;
              const updatedArtifact =
                artifactStreamingDataRef.current[artifact_value.id];
              const updatedValue = extractUpdatedValue(
                updatedArtifact,
                artifact_value,
              );
              if (updatedArtifact) {
                artifactStreamingDataRef.current[artifact_value.id] = {
                  ...artifactStreamingDataRef.current[artifact_value.id],
                  content: {
                    ...updatedArtifact.content,
                    value: {
                      ...updatedArtifact.content.value,
                      ...updatedValue,
                    },
                  },
                } as Artifact;
              }
              onStreamingArtifact?.(artifactStreamingDataRef.current);
              break;
            }
          }
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

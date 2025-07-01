import { Message, Answer } from "@/types/chat";
import {
  ArtifactDoneContent,
  ArtifactStartedContent,
  ArtifactValueContent,
  TextContent,
  UsageContent,
} from "@/types/chat";
import { STEP_MAP } from "@/types/agent";
import { Artifact } from "@/services/mock";
import { extractUpdatedValue } from "./artifact";
import { MessageProcessor } from "../../core/types";

export class AgentMessageProcessor implements MessageProcessor {
  processContent(streamingMessage: Message, answer: Answer): Message {
    switch (answer.content.type) {
      case "error":
        return {
          ...streamingMessage,
          content: answer.content.message,
        };
      case "text":
        return this.handleTextContent(streamingMessage, answer);
      case "artifact_started":
        return this.handleArtifactStarted(streamingMessage, answer.content);
      case "artifact_done":
        return this.handleArtifactDone(streamingMessage, answer.content);
      case "artifact_value":
        return this.handleArtifactValue(streamingMessage, answer.content);
      case "usage":
        return this.handleUsageContent(streamingMessage, answer.content);
      default:
        return streamingMessage;
    }
  }

  private handleTextContent(
    streamingMessage: Message,
    answer: Answer,
  ): Message {
    const { content: prevContent, references, steps } = streamingMessage;
    const shouldAddStep =
      answer.step &&
      Object.keys(STEP_MAP).includes(answer.step) &&
      steps.at(-1) !== answer.step;

    return {
      ...streamingMessage,
      content: prevContent + (answer.content as TextContent).content,
      references: answer.references
        ? [...references, ...answer.references]
        : references,
      steps: shouldAddStep && answer.step ? [...steps, answer.step] : steps,
    };
  }

  private handleArtifactStarted(
    streamingMessage: Message,
    content: ArtifactStartedContent,
  ): Message {
    const currentArtifacts = {
      ...streamingMessage.artifacts,
      [content.id]: {
        id: content.id,
        name: content.title,
        kind: content.kind.type,
        is_streaming: true,
        content: {
          type: content.kind.type,
          value: content.kind.value,
        },
      } as Artifact,
    };

    return {
      ...streamingMessage,
      artifacts: currentArtifacts,
    };
  }

  private handleArtifactDone(
    streamingMessage: Message,
    content: ArtifactDoneContent,
  ): Message {
    const currentArtifacts = {
      ...streamingMessage.artifacts,
      [content.id]: {
        ...streamingMessage.artifacts[content.id],
        is_streaming: false,
      } as Artifact,
    };

    return {
      ...streamingMessage,
      artifacts: currentArtifacts,
    };
  }

  private handleArtifactValue(
    streamingMessage: Message,
    content: ArtifactValueContent,
  ): Message {
    const currentArtifacts = streamingMessage.artifacts;
    const updatedArtifact = currentArtifacts[content.id];

    if (!updatedArtifact) {
      return streamingMessage;
    }

    const updatedValue = extractUpdatedValue(updatedArtifact, content);

    currentArtifacts[content.id] = {
      ...currentArtifacts[content.id],
      content: {
        ...updatedArtifact.content,
        value: {
          ...updatedArtifact.content.value,
          ...updatedValue,
        },
      },
    } as Artifact;

    return {
      ...streamingMessage,
      artifacts: currentArtifacts,
    };
  }

  private handleUsageContent(
    streamingMessage: Message,
    content: UsageContent,
  ): Message {
    return {
      ...streamingMessage,
      usage: content.usage,
    };
  }
}

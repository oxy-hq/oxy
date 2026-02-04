import { useQueryClient } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { MessagingService } from "./messagingService";
import type { MessageSender, SendMessageOptions, ThreadStore } from "./types";

export const useMessaging = (threadStore: ThreadStore, messageSender: MessageSender) => {
  const queryClient = useQueryClient();
  const messagingService = new MessagingService(messageSender, threadStore, queryClient);
  const { project, branchName } = useCurrentProjectBranch();

  const sendMessage = async (
    content: string | null,
    threadId: string,
    metadata?: {
      isPreview?: boolean;
      agentPathb64?: string;
    }
  ) => {
    const options: SendMessageOptions = {
      content,
      threadId,
      projectId: project.id,
      branchName,
      metadata
    };

    await messagingService.sendMessage(options);
  };

  return { sendMessage };
};

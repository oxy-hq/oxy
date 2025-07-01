import { useQueryClient } from "@tanstack/react-query";
import { ThreadStore, SendMessageOptions, MessageSender } from "./types";
import { MessagingService } from "./messagingService";

export const useMessaging = (
  threadStore: ThreadStore,
  messageSender: MessageSender,
) => {
  const queryClient = useQueryClient();
  const messagingService = new MessagingService(
    messageSender,
    threadStore,
    queryClient,
  );

  const sendMessage = async (
    content: string | null,
    threadId: string,
    isPreview?: boolean,
  ) => {
    const options: SendMessageOptions = {
      content,
      threadId,
      isPreview,
    };

    await messagingService.sendMessage(options);
  };

  return { sendMessage };
};

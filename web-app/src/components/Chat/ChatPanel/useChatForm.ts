import { FormEvent, RefObject, useCallback, useRef, useState } from "react";

import { useQueryClient } from "@tanstack/react-query";

import { toast } from "@/components/ui/Toast";
import queryKeys from "@/hooks/api/queryKey";

interface ChatFormProps {
  onSendChatMessage: (
    agentName: string,
    content: string,
    onSubmitQuestionSuccess: () => void
  ) => Promise<void>;
  formRef: RefObject<HTMLFormElement | null>;
}

const handleCreationError = (error: unknown, message: string) => {
  console.error("error", error);
  toast({
    title: "Error",
    description: message
  });
};

export const useChatForm = ({ onSendChatMessage, formRef }: ChatFormProps) => {
  const queryClient = useQueryClient();
  const [pending, setPending] = useState<boolean>(false);
  const starterRef = useRef<string>("");
  const isSubmittingRef = useRef<boolean>(false);

  const handleSubmit = useCallback(
    async (event: FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      if (isSubmittingRef.current) {
        return;
      }
      isSubmittingRef.current = true;
      const formData = new FormData(event.currentTarget);
      const starterMessage = starterRef.current;
      const content = starterMessage || (formData.get("content") as string);
      const agentPath = formData.get("agentPath") as string;

      if (!content) {
        return;
      }
      setPending(true);

      try {
        await onSendChatMessage(agentPath, content, () => {
          formRef.current?.reset();
        });
        queryClient.invalidateQueries({
          predicate: (query) =>
            queryKeys.conversation.all.every((key) => query.queryKey.includes(key))
        });
      } catch (error) {
        handleCreationError(error, "Error creating message");
      }

      setPending(false);
      isSubmittingRef.current = false;
    },
    [formRef, onSendChatMessage, queryClient]
  );

  return { pending, handleSubmit, starterRef };
};

import { FormEvent, RefObject, useCallback, useRef, useState } from "react";
import { toast } from "@/components/ui/Toast";
import useProjectPath from "@/stores/useProjectPath";

interface ChatFormProps {
  onSendChatMessage: (
    agentName: string,
    content: string,
    projectPath: string,
    onSubmitQuestionSuccess: () => void,
  ) => Promise<void>;
  formRef: RefObject<HTMLFormElement | null>;
}

const handleCreationError = (error: unknown, message: string) => {
  console.error("error", error);
  toast({
    title: "Error",
    description: message,
  });
};

export const useChatForm = ({ onSendChatMessage, formRef }: ChatFormProps) => {
  const [pending, setPending] = useState<boolean>(false);
  const starterRef = useRef<string>("");
  const isSubmittingRef = useRef<boolean>(false);
  const { projectPath } = useProjectPath();

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
        await onSendChatMessage(agentPath, content, projectPath, () => {
          formRef.current?.reset();
        });
      } catch (error) {
        handleCreationError(error, "Error creating message");
      }

      setPending(false);
      isSubmittingRef.current = false;
    },
    [formRef, onSendChatMessage, projectPath],
  );

  return { pending, handleSubmit, starterRef };
};

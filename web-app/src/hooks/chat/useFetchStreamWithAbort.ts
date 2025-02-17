import { useCallback, useRef } from "react";
import { ChatRequest, ChatType, Message } from "@/types/chat";
import { service } from "@/services/service";

const handleErrorWithAbort = (error: unknown) => {
  if ((error as Error).name === "AbortError") {
    return;
  }
  console.error("ERROR: CREATING/STREAMING MESSAGES", error);
  throw error;
};

export const useFetchStreamWithAbort = () => {
  const abortControllerRef = useRef<AbortController | null>(null);

  const fetchStreamWithAbort = useCallback(
    async (
      type: ChatType,
      onReadStream: (message: Message) => void,
      request: ChatRequest,
    ) => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }

      abortControllerRef.current = new AbortController();

      try {
        await service.chat(
          type,
          request,
          onReadStream,
          abortControllerRef.current.signal,
        );
      } catch (error) {
        handleErrorWithAbort(error);
      }
    },
    [],
  );

  const clearAbortController = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
  }, []);

  return { fetchStreamWithAbort, clearAbortController, abortControllerRef };
};

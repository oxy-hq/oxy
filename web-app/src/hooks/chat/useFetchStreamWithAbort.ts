import { useCallback, useRef } from "react";

import { readMessageFromStreamData } from "@/libs/utils/stream";
import { apiBaseURL } from "@/services/env";

const handleErrorWithAbort = (error: unknown) => {
  if ((error as Error).name === "AbortError") {
    return;
  }
  console.error("ERROR: CREATING/STREAMING MESSAGES", error);
  throw error;
};

type Message<T> = T;

export const useFetchStreamWithAbort = () => {
  const abortControllerRef = useRef<AbortController | null>(null);

  const fetchStreamWithAbort = useCallback(
    async <T>(url: string, onReadStream: (message: Message<T>) => void, options: RequestInit) => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }

      abortControllerRef.current = new AbortController();
      options.signal = abortControllerRef.current.signal;
      options.headers = {
        "Content-Type": "application/json"
      };

      try {
        const response = await fetch(apiBaseURL + url, options);
        if (response) {
          await readMessageFromStreamData(response, onReadStream);
        }
      } catch (error) {
        handleErrorWithAbort(error);
      }
    },
    []
  );

  const clearAbortController = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
  }, []);

  return { fetchStreamWithAbort, clearAbortController, abortControllerRef };
};


import { useState } from "react";

export interface ChatState {
  status: "success" | "error" | "loading" | "streaming" | "idle";
  errorMessage: string | null;
}

// Custom hook for managing chat state
export const useChatState = () => {
  const [chatState, setChatState] = useState<ChatState>({
    status: "idle",
    errorMessage: null,
  });

  const setChatStatus = (status: ChatState["status"]) => {
    if (status !== "error") {
      setChatState({ errorMessage: null, status });
      return;
    }
    setChatState((prev) => ({ ...prev, status }));
  };

  const setChatError = (errorMessage: string) => {
    setChatState({ status: "error", errorMessage });
  };

  return { chatState, setChatStatus, setChatError };
};

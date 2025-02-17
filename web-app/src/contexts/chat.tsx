import React, {
  createContext,
  Dispatch,
  ReactNode,
  useContext,
  useMemo,
} from "react";

import invariant from "invariant";

import { useChatActions } from "@/hooks/chat/useChatActions";
import { ChatState } from "@/hooks/chat/useChatState";
import { Message } from "@/types/chat";

interface IChatContextProps {
  messages: Message[];
  streamingNode: string | null;
  setMessages: Dispatch<React.SetStateAction<Message[]>>;
  onSendChatMessage: (
    agentName: string,
    content: string,
    projectPath: string,
    onSubmitQuestionSuccess: () => void,
  ) => Promise<void>;
  onStop: () => void;
  chatState: ChatState;
  updateMessage: (message: Message) => void;
  onResetDefaultMessages: (defaultMessages: Message[]) => void;
  startingMessageList: string[];
}

const ChatContext = createContext<IChatContextProps | undefined>(undefined);

export function ChatContextProvider({
  children,
  defaultMessages,
}: {
  children: ReactNode;
  defaultMessages?: Message[];
}) {
  const {
    messages,
    setMessages,
    streamingNode,
    onSendChatMessage,
    onStop,
    chatState,
    updateMessage,
    onResetDefaultMessages,
    startingMessageList,
  } = useChatActions(defaultMessages);

  const value = useMemo(
    () => ({
      messages,
      setMessages,
      streamingNode,
      onSendChatMessage,
      onStop,
      updateMessage,
      chatState,
      onResetDefaultMessages,
      startingMessageList,
    }),
    [
      messages,
      setMessages,
      streamingNode,
      onSendChatMessage,
      onStop,
      updateMessage,
      chatState,
      onResetDefaultMessages,
      startingMessageList,
    ],
  );

  return <ChatContext.Provider value={value}>{children}</ChatContext.Provider>;
}

export function useChatContextSelector<TSelected>(
  selector: (context: IChatContextProps) => TSelected,
): TSelected {
  const context = useContext(ChatContext);

  invariant(
    context,
    "`useChatContextSelector` must be used within a `ChatContextProvider`",
  );

  const selectedValues = useMemo(() => selector(context), [selector, context]);

  return selectedValues;
}

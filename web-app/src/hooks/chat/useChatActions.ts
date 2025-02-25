import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import dayjs from "dayjs";

import { getAgentNameFromPath } from "@/libs/utils/agent";
import { ChatRequest, ChatType, Message } from "@/types/chat";

import { useChatState } from "./useChatState";
import { useFetchStreamWithAbort } from "./useFetchStreamWithAbort";

const getStartingMessageList = (messages: Message[]) => {
  const dateList: string[] = [];
  const ids: string[] = [];
  // sort the messages without mutating the original array
  // we need to keep the original order of the messages to created_at desc
  const sortedMessages = [...messages].sort((a: Message, b: Message) => {
    return new Date(b.created_at || "") < new Date(a.created_at || "") ? 1 : -1;
  });

  sortedMessages.forEach((message: Message) => {
    if (!message.is_human) {
      return;
    }
    const formatDate = dayjs(message.created_at).format("DD/MM/YYYY");
    if (!dateList.includes(formatDate)) {
      dateList.push(formatDate);
      ids.push(message.id);
    }
  });
  return ids;
};

const updateOrPushMessageById = (message: Message, messages: Message[]) => {
  const newMessages = messages.filter((item) => item.id !== message.id);
  // append message to the first position
  // to maintain the order of the messages
  // created_at desc
  return [...newMessages, message];
};

export const useChatActions = (
  defaultMessages: Message[] = [],
  preview = false,
) => {
  const { fetchStreamWithAbort, clearAbortController } =
    useFetchStreamWithAbort();
  const { chatState, setChatStatus, setChatError } = useChatState();
  const [messages, setMessages] = useState<Message[]>(defaultMessages);
  const streamingNode = useRef<{ id: string; content: string } | null>(null);

  useEffect(() => {
    setMessages(defaultMessages);
  }, [defaultMessages]);

  // startingMessage is the first message of the date
  const startingMessageList = useMemo(
    () => getStartingMessageList(messages),
    [messages],
  );

  const onResetDefaultMessages = useCallback(
    (defaultMessages: Message[]) => {
      setMessages(defaultMessages);
      setChatStatus("idle");
      streamingNode.current = null;
    },
    [setChatStatus],
  );

  const handleChatAction = useCallback(
    async (
      type: ChatType,
      request: ChatRequest,
      onReadStream: (message: Message) => void,
    ) => {
      setChatStatus("loading");
      try {
        await fetchStreamWithAbort(type, onReadStream, request);
        setChatStatus("success");
      } catch (error) {
        setChatError("Failed to perform chat action.");
        console.error("🚀 ~ useChatActions ~ error:", error);
      } finally {
        clearAbortController();
        streamingNode.current = null;
      }
    },
    [clearAbortController, fetchStreamWithAbort, setChatError, setChatStatus],
  );

  const handleReceivedMessage = useCallback(
    (message: Message, onSubmitQuestionSuccess?: () => void) => {
      if (message.is_human && onSubmitQuestionSuccess) {
        onSubmitQuestionSuccess();
      }
      if (!message.is_human && streamingNode.current?.id !== message.id) {
        streamingNode.current = {
          id: message.id,
          content: "",
        };
        setChatStatus("streaming");
      }
      if (!message.is_human && streamingNode.current?.id === message.id) {
        streamingNode.current.content += message.content;
        message.content = streamingNode.current.content;
      }
      setMessages((prevMessages) =>
        updateOrPushMessageById(message, prevMessages),
      );
    },
    [setMessages, streamingNode, setChatStatus],
  );

  const onSendChatMessage = useCallback(
    async (
      agentPath: string,
      content: string,
      projectPath: string,
      onSubmitQuestionSuccess: () => void,
    ) => {
      await handleChatAction(
        preview ? "preview" : "chat",
        {
          question: content,
          agent: agentPath,
          title: getAgentNameFromPath(agentPath),
          memory: [],
          project_path: projectPath,
        },
        (message) => {
          handleReceivedMessage(message, onSubmitQuestionSuccess);
        },
      );
    },
    [handleChatAction, handleReceivedMessage, preview],
  );

  const updateMessage = useCallback(
    (message: Message) => {
      setMessages((prevMessages) =>
        updateOrPushMessageById(message, prevMessages),
      );
    },
    [setMessages],
  );

  const onStop = useCallback(() => {
    clearAbortController();
    streamingNode.current = null;
    setChatStatus("idle");
  }, [clearAbortController, setChatStatus]);

  return {
    messages,
    setMessages,
    streamingNode: streamingNode.current?.id || null,
    onSendChatMessage,
    onStop,
    chatState,
    updateMessage,
    onResetDefaultMessages,
    startingMessageList,
  };
};

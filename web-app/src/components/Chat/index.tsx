"use client";

import { css } from "styled-system/css";

import { ChatContextProvider } from "@/contexts/chat";
import { useChatMessages } from "@/hooks/api/useChatMessages";

import ChatPanel from "./ChatPanel";
import Messages from "./Messages";

const chatLayoutStyles = css({
  display: "flex",
  flexDirection: "column",
  flex: "1",
  width: "100%",
  overflow: "hidden",
  justifyContent: "space-between"
});

const chatMessagesWrapperStyles = css({
  display: "flex",
  overflowY: "auto",
  width: "100%",
  customScrollbar: true,
  scrollbarGutter: "stable both-edges",
  mt: "xl",
  px: "xl",
  smDown: {
    mt: "none",
    px: "md"
  },
  flexDir: "column-reverse"
});

const chatTextInputWrapperStyles = css({
  width: "100%",
  position: "relative",
  display: "flex",
  alignItems: "center",
  flexDirection: "column",
  gap: "sm",
  px: "xl",
  mt: "xs",
  mb: "xl"
});

interface ChatProps {
  agentPath: string;
}

export default function Chat({ agentPath }: ChatProps) {
  const { data: chatMessages } = useChatMessages(agentPath);

  return (
    <ChatContextProvider defaultMessages={chatMessages ?? []}>
      <div className={chatLayoutStyles}>
        <div className={chatMessagesWrapperStyles}>
          <Messages agentPath={agentPath} />
        </div>
        <div className={chatTextInputWrapperStyles}>
          <ChatPanel agentPath={agentPath} />
        </div>
      </div>
    </ChatContextProvider>
  );
}

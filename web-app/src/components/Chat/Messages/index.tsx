"use client";

import { memo, useEffect } from "react";

import { css } from "styled-system/css";

import { useToast } from "@/components/ui/Toast";
import { useChatContextSelector } from "@/contexts/chat";

import AgentInfo from "./AgentInfo";
import { ChatScrollAnchor } from "./ChatScrollAnchor";
import Message from "./Message";

const messageListStyle = css({
  display: "flex",
  flexDir: "column",
  width: "100%",
  gap: "5xl"
});

type IMessagesProps = { agentName: string };

function Messages({ agentName }: IMessagesProps) {
  const { toast } = useToast();

  const { chatState, messages, streamingNode, startingMessageList } = useChatContextSelector(
    (s) => ({
      streamingNode: s.streamingNode,
      chatState: s.chatState,
      messages: s.messages,
      startingMessageList: s.startingMessageList
    })
  );

  useEffect(() => {
    if (chatState.status === "error") {
      toast({
        title: "Error",
        description: chatState.errorMessage
      });
    }
  }, [chatState, toast]);

  return (
    <div className={messageListStyle}>
      <AgentInfo agentName={agentName} />
      <div className={css({ pb: "xl" })}>
        {messages.sort((a, b) => Number(a.created_at) - Number(b.created_at) ).map((message) => (
          <Message
            key={message.id}
            message={message}
            streamingNode={streamingNode}
            agentName={agentName}
            startingMessageList={startingMessageList}
          />
        ))}

        <ChatScrollAnchor trackVisibility={chatState.status === "streaming"} />
      </div>
    </div>
  );
}

export default memo(Messages);


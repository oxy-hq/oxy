import { css } from "styled-system/css";

import Button from "@/components/ui/Button";
import Icon from "@/components/ui/Icon";
import { useChatContextSelector } from "@/contexts/chat";
import { useEnterSubmit } from "@/hooks/useEnterSubmit";

import ChatTextArea from "./ChatTextArea";
import { useChatForm } from "./useChatForm";

const formStyles = css({
  maxW: "720px",
  mx: "auto",
  display: "flex",
  width: "100%"
});

const wrapperStyles = css({
  width: "100%",
  display: "flex",
  flexDirection: "column",
  gap: "md",
  alignItems: "center",
  justifyContent: "end"
});

export interface ChatPanelProps {
  agentName: string;
}

function ChatPanel({ agentName }: ChatPanelProps) {
  const { formRef, onKeyDown } = useEnterSubmit();

  const { streamingNode, onSendChatMessage, onStop, messages } = useChatContextSelector((s) => ({
    streamingNode: s.streamingNode,
    onSendChatMessage: s.onSendChatMessage,
    onStop: s.onStop,
    messages: s.messages
  }));

  const { pending, handleSubmit } = useChatForm({
    onSendChatMessage,
    formRef
  });

  const shouldShowStopButton = streamingNode !== null;

  return (
    <div className={wrapperStyles}>
      {shouldShowStopButton && (
        <Button onClick={onStop} content='iconText' variant='outline' size='large'>
          <Icon asset='close' /> Stop generating
        </Button>
      )}

      <form ref={formRef} onSubmit={handleSubmit} className={formStyles}>
        <input hidden name='agentName' defaultValue={agentName} />
        <ChatTextArea
          onKeyDown={onKeyDown}
          hasMessage={!!messages.length}
          pending={pending}
          botName={agentName}
        />
      </form>
    </div>
  );
}

export default ChatPanel;


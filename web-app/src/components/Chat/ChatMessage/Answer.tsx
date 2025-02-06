import { memo, useMemo } from "react";

import { css } from "styled-system/css";
import { stack } from "styled-system/patterns";

import { Message } from "@/types/chat";

import AnswerContent from "./AnswerContent";
import LoadingAnimation from "./LoadingAnimation";
import Metadata from "./Metadata";

type Props = {
  message: Message;
  isStreaming: boolean;
  agentName: string;
};

const containerStyles = css({
  width: "100%",
  display: "flex",
  justifyContent: "flex-start",
  maxW: {
    base: "350px",
    sm: "720px",
  },
  marginX: "auto",
});

const answerWrapStyle = stack({
  gap: "xl",
  flexDirection: "column",
  w: "100%",
});

const headerStyle = css({
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
});

const loadingStyles = css({
  marginLeft: "sm",
  marginTop: "xs",
});

function Answer({ message, isStreaming, agentName }: Props) {
  const renderAnswer = useMemo(() => {
    if (!message.content && isStreaming) {
      return (
        <div className={loadingStyles}>
          <LoadingAnimation />
        </div>
      );
    }

    return <AnswerContent content={message.content} />;
  }, [message, isStreaming]);

  return (
    <div className={containerStyles}>
      <div className={answerWrapStyle}>
        <div className={headerStyle}>
          <Metadata agentName={agentName} time={message.created_at} />
        </div>
        {renderAnswer}
      </div>
    </div>
  );
}

export default memo(Answer, arePropsEqual);

function arePropsEqual(oldProps: Props, newProps: Props) {
  return (
    oldProps.message.content === newProps.message.content &&
    oldProps.isStreaming === newProps.isStreaming
  );
}

import { css } from "styled-system/css";

import Question from "@/components/Chat/ChatMessage/Question";
import Text from "@/components/ui/Typography/Text";
import { formatStartingDate } from "@/libs/utils/date";
import { Message as MessageModel } from "@/types/chat";

import Answer from "../ChatMessage/Answer";

const wrapStyle = css({
  display: "flex",
  flexDir: "column",
  width: "100%",
  gap: "xl",
});

const timeStyles = css({
  color: "text.secondary",
  py: "padding.paddingSM",
});

const dateWrapperStyle = css({
  display: "flex",
  justifyContent: "center",
});

interface MessageProps {
  message: MessageModel;
  streamingNode: string | null;
  agentName: string;
  startingMessageList: string[];
}

function Message({
  message,
  streamingNode,
  agentName,
  startingMessageList,
}: MessageProps) {
  const renderDateOfMessage = () => {
    if (startingMessageList.includes(message.id)) {
      const timeFormat = formatStartingDate(message.created_at);
      return (
        <Text className={timeStyles} variant="label14Regular">
          {timeFormat}
        </Text>
      );
    }
  };

  return (
    <div className={wrapStyle}>
      <div className={dateWrapperStyle}>{renderDateOfMessage()}</div>
      {!message.is_human ? (
        <Answer
          key={message.id}
          message={message}
          isStreaming={streamingNode === message.id}
          agentName={agentName}
        />
      ) : (
        <Question key={message.id} content={message.content} />
      )}
    </div>
  );
}

export default Message;

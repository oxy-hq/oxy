import { useParams } from "react-router-dom";
import { css } from "styled-system/css";

import Chat from "@/components/Chat";

const contentStyles = css({
  marginTop: {
    base: "52px",
    sm: "md"
  },
  display: "flex",
  flex: "1",
  minW: "0",
  my: {
    base: "none",
    sm: "md"
  },
  mr: {
    base: "none",
    sm: "md"
  },
  border: {
    base: "none",
    sm: "1px solid token(colors.border.primary)"
  },
  borderRadius: {
    base: "none",
    sm: "full"
  },
  overflow: "hidden",
  backgroundColor: "background.primary"
});

const chatStyles = css({
  display: "flex",
  flex: "1",
  flexDir: "column",
  overflow: "hidden"
});

const ChatPage = () => {
  const { id: agentPathBase64 } = useParams();

  const agentPath = atob(agentPathBase64 ?? "");

  return (
    <div className={contentStyles}>
      <div className={chatStyles}>
        <Chat agentPath={agentPath} />
      </div>
    </div>
  );
};

export default ChatPage;

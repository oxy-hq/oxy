import { useParams } from "react-router-dom";
import { css } from "styled-system/css";

import Text from "@/components/ui/Typography/Text";
import useProjectPath from "@/stores/useProjectPath";
import Chat from "@/components/Chat";

const wrapperStyles = css({
  width: "100%",
  height: "100%",
  display: "flex",
  flexDir: "column",
});

const headerStyles = css({
  padding: "sm",
  border: "1px solid",
  borderColor: "neutral.border.colorBorderSecondary",
  backgroundColor: "neutral.bg.colorBg",
});

const chatStyles = css({
  display: "flex",
  flex: "1",
  flexDir: "column",
  overflow: "hidden",
});

const AgentPage = () => {
  const pathb64 = useParams<{ pathb64: string }>().pathb64!;
  const agentPath = atob(pathb64);
  const projectPath = useProjectPath((state) => state.projectPath);
  const relativePath = agentPath.replace(projectPath, "").replace(/^\//, "");

  return (
    <div className={wrapperStyles}>
      <div className={headerStyles}>
        <Text variant="bodyBaseMedium">{relativePath}</Text>
      </div>
      <div className={chatStyles}>
        <Chat key={relativePath} agentPath={relativePath} />
      </div>
    </div>
  );
};

export default AgentPage;

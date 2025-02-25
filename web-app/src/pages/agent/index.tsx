import { useParams } from "react-router-dom";
import { css } from "styled-system/css";

import Text from "@/components/ui/Typography/Text";
import useProjectPath from "@/stores/useProjectPath";
import Chat from "@/components/Chat";
import { TabPanel } from "@/components/ui/Tabs";
import { TabList } from "@/components/ui/Tabs";
import { Tab } from "@/components/ui/Tabs";
import { Tabs } from "@/components/ui/Tabs";
import { useState } from "react";
import AgentEditor from "@/components/AgentEditor";

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

const tabListStyles = css({
  padding: "md",
});

const AgentPage = () => {
  const pathb64 = useParams<{ pathb64: string }>().pathb64!;
  const agentPath = atob(pathb64);
  const projectPath = useProjectPath((state) => state.projectPath);
  const relativePath = agentPath.replace(projectPath, "").replace(/^\//, "");
  const [selectedTab, setSelectedTab] = useState("chat");

  const handleTabChange = (value: string) => {
    setSelectedTab(value);
  };

  return (
    <div className={wrapperStyles}>
      <div className={headerStyles}>
        <Text variant="bodyBaseMedium">{relativePath}</Text>
      </div>

      <div
        className={css({
          flex: "1",
          display: "flex",
          flexDir: "row",
          overflow: "hidden",
        })}
      >
        <div
          className={css({
            flex: "1",
            display: "flex",
            flexDir: "column",
            overflow: "hidden",
          })}
        >
          <Tabs defaultValue="chat" onChange={handleTabChange}>
            <TabList className={tabListStyles}>
              <Tab value="chat">Chat</Tab>
              <Tab value="build">Build</Tab>
            </TabList>
            <TabPanel value="chat" className={chatStyles}>
              <Chat key={relativePath} agentPath={relativePath} />
            </TabPanel>
            <TabPanel value="build" className={chatStyles}>
              <Chat key={relativePath} agentPath={relativePath} preview />
            </TabPanel>
          </Tabs>
        </div>

        {selectedTab === "build" && <AgentEditor path={agentPath} />}
      </div>
    </div>
  );
};

const AgentPageWrapper = () => {
  const { pathb64 } = useParams();
  return <AgentPage key={pathb64} />;
};

export default AgentPageWrapper;

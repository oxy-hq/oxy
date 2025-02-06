import { css } from "styled-system/css";

import useAgents from "@/hooks/api/useAgents";

import AgentList from "./AgentList";
import Header from "./Header";

const homeStyles = css({
  display: "flex",
  flex: "1",
  flexDir: "column",
  overflow: "hidden",
  pt: "5xl",
  pb: "xl",
  px: "60px",
  gap: "5xl",
});

const Home = () => {
  const { data: agents, isLoading } = useAgents(true, false, "always");
  return (
    <div className={homeStyles}>
      <Header />
      <AgentList agents={agents} isLoading={isLoading} />
    </div>
  );
};

export default Home;

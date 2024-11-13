import { hstack, vstack } from "styled-system/patterns";

import { Agent } from "@/types/chat";

import Text from "../ui/Typography/Text";
import { AgentCard, AgentCardSkeleton } from "./AgentCard";

export default function AgentList({ agents, isLoading }: { agents?: Agent[]; isLoading: boolean }) {
  return (
    <div
      className={vstack({
        alignItems: "start",
        gap: "xl",
        width: "100%",
        overflow: "hidden"
      })}
    >
      <div
        className={hstack({
          gap: "xs"
        })}
      >
        <Text variant='label16Medium' color="primary">Agents</Text>
      </div>

      <div
        className={vstack({
          gap: "lg",
          width: "100%",
          overflow: "auto",
          customScrollbar: true
        })}
      >
        {isLoading &&
          Array(5)
            .fill(null)
            // eslint-disable-next-line sonarjs/no-array-index-key
            .map((_, index) => <AgentCardSkeleton key={index} />)}
        {agents && (
          <>{agents.map((agent: Agent) => agent && <AgentCard key={agent.name} agent={agent} />)}</>
        )}
      </div>
    </div>
  );
}


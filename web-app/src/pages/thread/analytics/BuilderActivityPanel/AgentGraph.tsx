import { useMemo } from "react";

import type { BuilderProposedChange } from "@/hooks/useBuilderActivity";
import { GenericGraph } from "./GenericGraph";
import { type AgentConfig, diffAgentItems } from "./types";

export const AgentGraph = ({
  change,
  oldAgent,
  newAgent
}: {
  change: BuilderProposedChange;
  oldAgent: AgentConfig | null;
  newAgent: AgentConfig;
}) => {
  const diffs = useMemo(() => diffAgentItems(oldAgent, newAgent), [oldAgent, newAgent]);
  const changedItems = useMemo(() => diffs.filter((d) => d.status !== "unchanged"), [diffs]);
  const toolCount = (newAgent.tools ?? []).length;
  const ctxCount = (newAgent.context ?? []).length;
  const subtitle = [
    `${toolCount} tools`,
    ctxCount > 0 ? `${ctxCount} context` : null,
    newAgent.model
  ]
    .filter(Boolean)
    .join(" · ");
  return (
    <GenericGraph
      change={change}
      graphLabel='Agent Graph'
      rootLabel='Agent'
      rootTitle={newAgent.name ?? "untitled"}
      rootSubtitle={subtitle}
      changedItems={changedItems}
    />
  );
};

import { useMemo } from "react";
import useAgents from "@/hooks/api/agents/useAgents";
import { getAgentNameFromPath } from "@/libs/utils/string";
import type { Agent } from "./AgentsDropdown";

/** Public agents mapped to the picker's `Agent` shape, sorted by name.
 *  Shared by the visible dropdown and the locked composer so both derive
 *  the option set the same way. */
export function useAgentOptions() {
  const { data: agents, isPending, isSuccess } = useAgents();
  const agentOptions = useMemo<Agent[]>(
    () =>
      agents
        ?.filter((agent) => agent.public)
        ?.map((agent) => ({
          id: agent.path,
          isAnalytics: agent.path.endsWith(".agentic.yml") || agent.path.endsWith(".agentic.yaml"),
          name: agent.name ?? getAgentNameFromPath(agent.path)
        }))
        .sort((a, b) => a.name.localeCompare(b.name)) ?? [],
    [agents]
  );
  return { agentOptions, isPending, isSuccess };
}

/** Resolve the agent to use when the picker is hidden: the preferred path if
 *  present, otherwise the first analytics (`.agentic.yml`) agent, otherwise the
 *  first agent. */
export function resolveDefaultAgent(options: Agent[], preferAgentPath?: string): Agent | null {
  if (preferAgentPath) {
    const preferred = options.find((a) => a.id === preferAgentPath);
    if (preferred) return preferred;
  }
  return options.find((a) => a.isAnalytics) ?? options[0] ?? null;
}

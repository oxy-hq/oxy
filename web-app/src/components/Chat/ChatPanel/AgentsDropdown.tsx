import { Bot, Loader2, Route } from "lucide-react";
import { useEffect, useMemo } from "react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import useAgents from "@/hooks/api/agents/useAgents";
import { getAgentNameFromPath } from "@/libs/utils/string";

export type Agent = {
  id: string;
  isAgentic: boolean;
  isAnalytics: boolean;
  name: string;
};

type Props = {
  onSelect: (agent: Agent) => void;
  agentSelected: Agent | null;
  disabled?: boolean;
};

const AgentsDropdown = ({ onSelect, agentSelected, disabled = false }: Props) => {
  const { data: agents, isPending, isSuccess } = useAgents();

  const agentOptions = useMemo(
    () =>
      agents
        ?.filter((agent) => agent.public)
        ?.map((agent) => ({
          id: agent.path,
          isAgentic: agent.path.endsWith(".aw.yaml") || agent.path.endsWith(".aw.yml"),
          isAnalytics: agent.path.endsWith(".agentic.yml") || agent.path.endsWith(".agentic.yaml"),
          name: agent.name ?? getAgentNameFromPath(agent.path)
        }))
        .sort((a, b) => a.name.localeCompare(b.name)) ?? [],
    [agents]
  );

  useEffect(() => {
    if (isSuccess && agents && agents.length > 0 && !agentSelected) {
      onSelect(agentOptions[0]);
    }
  }, [isSuccess, agents, agentOptions, onSelect, agentSelected]);

  return (
    <Select
      value={agentSelected?.id ?? ""}
      onValueChange={(id) => {
        const agent = agentOptions.find((a) => a.id === id);
        if (agent) onSelect(agent);
      }}
      disabled={isPending || disabled}
    >
      <SelectTrigger
        size='sm'
        className='w-auto border-none shadow-none'
        data-testid='agent-selector-button'
      >
        {isPending ? (
          <Loader2 className='size-4 animate-spin' />
        ) : (
          <SelectValue placeholder='Select agent' />
        )}
      </SelectTrigger>
      <SelectContent>
        {agentOptions.map((item) => (
          <SelectItem className='cursor-pointer' key={item.id} value={item.id}>
            {item.name.includes("routing") ? (
              <Route className='size-4' />
            ) : (
              <Bot className='size-4' />
            )}
            {item.name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
};

export default AgentsDropdown;

import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenuContent,
  DropdownMenuCheckboxItem,
  DropdownMenuTrigger,
} from "@/components/ui/shadcn/dropdown-menu";
import { DropdownMenu } from "@/components/ui/shadcn/dropdown-menu";
import useAgents from "@/hooks/api/useAgents";
import { ChevronDown, Loader2 } from "lucide-react";
import { useEffect, useMemo } from "react";
import { getAgentNameFromPath } from "@/libs/utils/string";

export type Agent = {
  id: string;
  name: string;
};

const AgentsDropdown = ({
  onSelect,
  agent,
}: {
  onSelect: (agent: Agent) => void;
  agent: Agent | null;
}) => {
  const { data: agents, isLoading, isSuccess } = useAgents();

  const agentOptions = useMemo(
    () =>
      agents
        ?.map((agentPath) => ({
          id: agentPath,
          name: getAgentNameFromPath(agentPath),
        }))
        .sort((a, b) => a.name.localeCompare(b.name)) ?? [],
    [agents],
  );

  useEffect(() => {
    if (isSuccess && agents && agents.length > 0 && !agent) {
      onSelect(agentOptions[0]);
    }
  }, [isSuccess, agents, agentOptions, onSelect, agent]);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger>
        <Button disabled={isLoading} variant="outline">
          <span>{agent?.name}</span>
          {isLoading ? <Loader2 className="animate-spin" /> : <ChevronDown />}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent>
        {agentOptions.map((item) => (
          <DropdownMenuCheckboxItem
            key={item.id}
            checked={item.id === agent?.id}
            onCheckedChange={() => onSelect(item)}
          >
            {item.name}
          </DropdownMenuCheckboxItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
};

export default AgentsDropdown;

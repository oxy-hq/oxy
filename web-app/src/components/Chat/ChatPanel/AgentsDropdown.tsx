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

type Props = {
  onSelect: (agent: Agent) => void;
  agentSelected: Agent | null;
  disabled?: boolean;
};

const AgentsDropdown = ({
  onSelect,
  agentSelected,
  disabled = false,
}: Props) => {
  const { data: agents, isPending, isSuccess } = useAgents();

  const agentOptions = useMemo(
    () =>
      agents
        ?.filter((agent) => agent.public)
        ?.map((agent) => ({
          id: agent.path,
          name: agent.name ?? getAgentNameFromPath(agent.path),
        }))
        .sort((a, b) => a.name.localeCompare(b.name)) ?? [],
    [agents],
  );

  useEffect(() => {
    if (isSuccess && agents && agents.length > 0 && !agentSelected) {
      onSelect(agentOptions[0]);
    }
  }, [isSuccess, agents, agentOptions, onSelect, agentSelected]);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger disabled={isPending || disabled} asChild>
        <Button
          disabled={isPending || disabled}
          variant="outline"
          className="bg-sidebar-background border-sidebar-background"
        >
          <span>{agentSelected?.name}</span>
          {isPending ? <Loader2 className="animate-spin" /> : <ChevronDown />}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent className="customScrollbar">
        {agentOptions.map((item) => (
          <DropdownMenuCheckboxItem
            key={item.id}
            checked={item.id === agentSelected?.id}
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

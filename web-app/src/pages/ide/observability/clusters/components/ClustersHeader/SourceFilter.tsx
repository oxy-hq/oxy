import { ChevronDown, Loader2 } from "lucide-react";
import { useMemo } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuSeparator,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";
import useAgents from "@/hooks/api/agents/useAgents";
import { getAgentNameFromPath } from "@/libs/utils/string";

type SourceOption = {
  id: string;
  name: string;
  type: "all" | "agent";
};

type Props = {
  onSelect: (source: string | undefined) => void;
  selectedSource: string | undefined;
};

const SourceFilter = ({ onSelect, selectedSource }: Props) => {
  const { data: agents, isPending } = useAgents();

  const options = useMemo(() => {
    const allOption: SourceOption = {
      id: "all",
      name: "All agents",
      type: "all"
    };

    const agentOptions: SourceOption[] =
      agents
        ?.filter((agent) => agent.public)
        ?.map((agent) => ({
          id: agent.path,
          name: agent.name ?? getAgentNameFromPath(agent.path),
          type: "agent" as const
        }))
        .sort((a, b) => a.name.localeCompare(b.name)) ?? [];

    return { allOption, agentOptions };
  }, [agents]);

  const selectedOption = useMemo(() => {
    if (!selectedSource) return options.allOption;
    const agentMatch = options.agentOptions.find((opt) => opt.id === selectedSource);
    if (agentMatch) return agentMatch;

    return options.allOption;
  }, [selectedSource, options]);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant='outline' className='w-40'>
          <span className='truncate'>{selectedOption.name}</span>
          {isPending ? <Loader2 className='animate-spin' /> : <ChevronDown />}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent className='customScrollbar'>
        <DropdownMenuCheckboxItem
          checked={selectedOption.id === "all"}
          onCheckedChange={() => onSelect(undefined)}
        >
          {options.allOption.name}
        </DropdownMenuCheckboxItem>

        {options.agentOptions.length > 0 && (
          <>
            <DropdownMenuSeparator />
            {options.agentOptions.map((item) => (
              <DropdownMenuCheckboxItem
                key={item.id}
                checked={item.id === selectedSource}
                onCheckedChange={() => onSelect(item.id)}
              >
                {item.name}
              </DropdownMenuCheckboxItem>
            ))}
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
};

export default SourceFilter;

import { useMemo } from "react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectSeparator,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
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
  const { data: agents } = useAgents();

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

  return (
    <Select
      value={selectedSource ?? "all"}
      onValueChange={(v) => onSelect(v === "all" ? undefined : v)}
    >
      <SelectTrigger size='sm'>
        <SelectValue placeholder='All agents' />
      </SelectTrigger>
      <SelectContent>
        <SelectItem className='cursor-pointer' value='all'>
          {options.allOption.name}
        </SelectItem>

        {options.agentOptions.length > 0 && (
          <>
            <SelectSeparator />
            {options.agentOptions.map((item) => (
              <SelectItem className='cursor-pointer' key={item.id} value={item.id}>
                {item.name}
              </SelectItem>
            ))}
          </>
        )}
      </SelectContent>
    </Select>
  );
};

export default SourceFilter;

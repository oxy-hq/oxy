import { Bot, ChevronDown, Route } from "lucide-react";
import { useEffect, useMemo } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { Switch } from "@/components/ui/shadcn/switch";
import useAgents from "@/hooks/api/agents/useAgents";
import { getAgentNameFromPath } from "@/libs/utils/string";
import type { ThinkingMode } from "@/services/api/analytics";

export type Agent = {
  id: string;
  isAgentic: boolean;
  isAnalytics: boolean;
  name: string;
};

type Props = {
  onSelect: (agent: Agent) => void;
  agentSelected: Agent | null;
  thinkingMode: ThinkingMode;
  onThinkingModeChange: (mode: ThinkingMode) => void;
  disabled?: boolean;
};

const AgentsDropdown = ({
  onSelect,
  agentSelected,
  thinkingMode,
  onThinkingModeChange,
  disabled = false
}: Props) => {
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
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant='ghost'
          size='sm'
          className='h-8 gap-2 border-none bg-input/30 px-3 shadow-none'
          disabled={isPending || disabled}
          data-testid='agent-selector-button'
        >
          {isPending ? (
            <Spinner />
          ) : (
            <>
              {agentSelected?.name.includes("routing") ? (
                <Route className='size-4' />
              ) : (
                <Bot className='size-4' />
              )}
              <span>{agentSelected?.name ?? "Select agent"}</span>
              {thinkingMode === "extended_thinking" && (
                <span className='text-muted-foreground text-xs'>Extended</span>
              )}
              <ChevronDown className='opacity-50' />
            </>
          )}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align='end'>
        {agentOptions.map((item) => (
          <DropdownMenuItem
            className='cursor-pointer'
            key={item.id}
            onClick={() => onSelect(item)}
            data-highlighted={agentSelected?.id === item.id}
          >
            {item.name.includes("routing") ? (
              <Route className='size-4' />
            ) : (
              <Bot className='size-4' />
            )}
            {item.name}
          </DropdownMenuItem>
        ))}
        {agentSelected?.isAnalytics && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              className='flex cursor-default items-center justify-between focus:bg-transparent'
              onSelect={(e) => e.preventDefault()}
            >
              <span>Extended Thinking</span>
              <Switch
                checked={thinkingMode === "extended_thinking"}
                onCheckedChange={(checked) =>
                  onThinkingModeChange(checked ? "extended_thinking" : "auto")
                }
              />
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
};

export default AgentsDropdown;

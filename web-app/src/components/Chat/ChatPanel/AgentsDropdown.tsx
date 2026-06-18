import { Bot, ChevronDown } from "lucide-react";
import { useEffect } from "react";
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
import type { ThinkingMode } from "@/services/api/analytics";
import { useAgentOptions } from "./useAgentOptions";

export type Agent = {
  id: string;
  isAnalytics: boolean;
  name: string;
};

type Props = {
  onSelect: (agent: Agent) => void;
  agentSelected: Agent | null;
  /** When set, auto-select this agent instead of the first in the list */
  preferAgentPath?: string;
  thinkingMode: ThinkingMode;
  onThinkingModeChange: (mode: ThinkingMode) => void;
  disabled?: boolean;
};

const AgentsDropdown = ({
  onSelect,
  agentSelected,
  preferAgentPath,
  thinkingMode,
  onThinkingModeChange,
  disabled = false
}: Props) => {
  const { agentOptions, isPending, isSuccess } = useAgentOptions();

  useEffect(() => {
    if (isSuccess && agentOptions.length > 0 && !agentSelected) {
      const preferred = preferAgentPath
        ? agentOptions.find((a) => a.id === preferAgentPath)
        : undefined;
      onSelect(preferred ?? agentOptions[0]);
    }
  }, [isSuccess, agentOptions, onSelect, agentSelected, preferAgentPath]);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant='outline'
          size='sm'
          className='h-8 min-w-0 max-w-full gap-2 px-3'
          disabled={isPending || disabled}
          data-testid='agent-selector-button'
        >
          {isPending ? (
            <Spinner />
          ) : (
            <>
              <Bot className='size-4 shrink-0' />
              <span className='truncate'>{agentSelected?.name ?? "Select agent"}</span>
              {thinkingMode === "extended_thinking" && (
                <span className='hidden text-muted-foreground text-xs md:inline'>Extended</span>
              )}
              <ChevronDown className='shrink-0 opacity-50' />
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
            <Bot className='size-4' />
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

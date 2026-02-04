import { ChevronDown, Loader2 } from "lucide-react";
import { useEffect, useMemo } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";
import useWorkflows from "@/hooks/api/workflows/useWorkflows";

export type WorkflowOption = {
  id: string;
  name: string;
};

type Props = {
  onSelect: (workflow: WorkflowOption) => void;
  workflow: WorkflowOption | null;
  disabled?: boolean;
};

const WorkflowsDropdown = ({ onSelect, workflow, disabled = false }: Props) => {
  const { data: workflows, isLoading, isSuccess } = useWorkflows();

  const workflowOptions = useMemo(
    () =>
      workflows
        ?.map((workflow) => ({
          id: workflow.path ?? "",
          name: workflow.name
        }))
        .sort((a, b) => a.name.localeCompare(b.name)) ?? [],
    [workflows]
  );

  useEffect(() => {
    if (isSuccess && workflows && workflows.length > 0 && !workflow) {
      onSelect(workflowOptions[0]);
    }
  }, [isSuccess, workflows, workflowOptions, onSelect, workflow]);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger disabled={isLoading || disabled} asChild>
        <Button
          disabled={isLoading || disabled}
          variant='outline'
          className='border-sidebar-background bg-sidebar-background'
          data-testid='workflow-selector-button'
        >
          <span>{workflow?.name}</span>
          {isLoading ? <Loader2 className='animate-spin' /> : <ChevronDown />}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent className='customScrollbar'>
        {workflowOptions.map((item) => (
          <DropdownMenuCheckboxItem
            key={item.id}
            checked={item.id === workflow?.id}
            onCheckedChange={() => onSelect(item)}
          >
            {item.name}
          </DropdownMenuCheckboxItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
};

export default WorkflowsDropdown;

import { Loader2, Workflow } from "lucide-react";
import { useEffect, useMemo } from "react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
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
    <Select
      value={workflow?.id ?? ""}
      onValueChange={(id) => {
        const option = workflowOptions.find((w) => w.id === id);
        if (option) onSelect(option);
      }}
      disabled={isLoading || disabled}
    >
      <SelectTrigger
        size='sm'
        className='w-auto border-none shadow-none'
        data-testid='workflow-selector-button'
      >
        {isLoading ? (
          <Loader2 className='size-4 animate-spin' />
        ) : (
          <SelectValue placeholder='Select procedure' />
        )}
      </SelectTrigger>
      <SelectContent className='customScrollbar'>
        {workflowOptions.map((item) => (
          <SelectItem className='cursor-pointer' key={item.id} value={item.id}>
            <Workflow className='size-4' />
            {item.name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
};

export default WorkflowsDropdown;

import { Workflow } from "lucide-react";
import { useEffect, useMemo } from "react";

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useAgenticWorkflowFiles } from "@/hooks/api/agentic-workflows/useAgenticWorkflows";

export type WorkflowOption = {
  /** path_b64 — used as both the route param and the select's value. */
  id: string;
  name: string;
};

type Props = {
  onSelect: (workflow: WorkflowOption) => void;
  workflow: WorkflowOption | null;
  disabled?: boolean;
};

/** Strip directories and known suffixes — `path/to/foo.workflow.yml` → `foo`. */
const displayName = (path: string): string => {
  const stem = path.split("/").pop() ?? path;
  return stem.replace(/\.(workflow|procedure|automation)\.ya?ml$/i, "");
};

/** Pure presentational view of the dropdown. */
export const WorkflowsDropdownView = ({
  options,
  selectedId,
  onChange,
  isLoading,
  disabled
}: {
  options: WorkflowOption[];
  selectedId: string;
  onChange: (option: WorkflowOption) => void;
  isLoading: boolean;
  disabled: boolean;
}) => (
  <Select
    value={selectedId}
    onValueChange={(id) => {
      const option = options.find((w) => w.id === id);
      if (option) onChange(option);
    }}
    disabled={isLoading || disabled}
  >
    <SelectTrigger
      size='sm'
      className='w-auto border-none shadow-none'
      data-testid='workflow-selector-button'
    >
      {isLoading ? <Spinner /> : <SelectValue placeholder='Select procedure' />}
    </SelectTrigger>
    <SelectContent>
      {options.map((item) => (
        <SelectItem className='cursor-pointer' key={item.id} value={item.id}>
          <Workflow className='size-4' />
          {item.name}
        </SelectItem>
      ))}
    </SelectContent>
  </Select>
);

const WorkflowsDropdown = ({ onSelect, workflow, disabled = false }: Props) => {
  const { data, isLoading, isSuccess } = useAgenticWorkflowFiles();

  const workflowOptions: WorkflowOption[] = useMemo(
    () =>
      (data ?? [])
        .map((file) => ({ id: file.path_b64, name: displayName(file.path) }))
        .sort((a, b) => a.name.localeCompare(b.name)),
    [data]
  );

  useEffect(() => {
    if (isSuccess && workflowOptions.length > 0 && !workflow) {
      onSelect(workflowOptions[0]);
    }
  }, [isSuccess, workflowOptions, onSelect, workflow]);

  return (
    <WorkflowsDropdownView
      options={workflowOptions}
      selectedId={workflow?.id ?? ""}
      onChange={onSelect}
      isLoading={isLoading}
      disabled={disabled}
    />
  );
};

export default WorkflowsDropdown;

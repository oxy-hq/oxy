import { Workflow as Automation } from "lucide-react";
import { useEffect, useMemo } from "react";

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useAgenticAutomationFiles } from "@/hooks/api/agentic-automations/useAgenticAutomations";

export type AutomationOption = {
  /** path_b64 — used as both the route param and the select's value. */
  id: string;
  name: string;
};

type Props = {
  onSelect: (automation: AutomationOption) => void;
  automation: AutomationOption | null;
  disabled?: boolean;
};

/** Strip directories and known suffixes — `path/to/foo.automation.yml` → `foo`. */
const displayName = (path: string): string => {
  const stem = path.split("/").pop() ?? path;
  return stem.replace(/\.(workflow|procedure|automation)\.ya?ml$/i, "");
};

/** Pure presentational view of the dropdown. */
const AutomationsDropdownView = ({
  options,
  selectedId,
  onChange,
  isLoading,
  disabled
}: {
  options: AutomationOption[];
  selectedId: string;
  onChange: (option: AutomationOption) => void;
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
      data-testid='automation-selector-button'
    >
      {isLoading ? <Spinner /> : <SelectValue placeholder='Select automation' />}
    </SelectTrigger>
    <SelectContent>
      {options.map((item) => (
        <SelectItem className='cursor-pointer' key={item.id} value={item.id}>
          <Automation className='size-4' />
          {item.name}
        </SelectItem>
      ))}
    </SelectContent>
  </Select>
);

const AutomationsDropdown = ({ onSelect, automation, disabled = false }: Props) => {
  const { data, isLoading, isSuccess } = useAgenticAutomationFiles();

  const automationOptions: AutomationOption[] = useMemo(
    () =>
      (data ?? [])
        .map((file) => ({ id: file.path_b64, name: displayName(file.path) }))
        .sort((a, b) => a.name.localeCompare(b.name)),
    [data]
  );

  useEffect(() => {
    if (isSuccess && automationOptions.length > 0 && !automation) {
      onSelect(automationOptions[0]);
    }
  }, [isSuccess, automationOptions, onSelect, automation]);

  return (
    <AutomationsDropdownView
      options={automationOptions}
      selectedId={automation?.id ?? ""}
      onChange={onSelect}
      isLoading={isLoading}
      disabled={disabled}
    />
  );
};

export default AutomationsDropdown;

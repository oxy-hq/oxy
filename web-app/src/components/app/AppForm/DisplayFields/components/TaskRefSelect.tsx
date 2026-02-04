import type React from "react";
import { useMemo } from "react";
import { useFormContext } from "react-hook-form";
import { Combobox } from "@/components/ui/shadcn/combobox";
import type { AppFormData } from "../../index";

interface TaskRefSelectProps {
  value: string | undefined;
  onChange: (value: string) => void;
  placeholder?: string;
}

export const TaskRefSelect: React.FC<TaskRefSelectProps> = ({
  value,
  onChange,
  placeholder = "Select task..."
}) => {
  const { watch } = useFormContext<AppFormData>();
  const tasks = watch("tasks");

  const taskItems = useMemo(() => {
    const items = (tasks || [])
      .filter((task) => task?.name)
      .map((task) => ({
        value: task.name!,
        label: task.name!,
        searchText: `${task.name} ${task.type || ""}`.toLowerCase()
      }));

    if (value && !items.some((item) => item.value === value)) {
      items.unshift({
        value,
        label: value,
        searchText: value.toLowerCase()
      });
    }

    return items;
  }, [tasks, value]);

  if (taskItems.length === 0) {
    return (
      <div className='flex h-10 items-center rounded-md border bg-muted px-3'>
        <span className='text-muted-foreground text-sm'>No tasks defined</span>
      </div>
    );
  }

  return (
    <Combobox
      items={taskItems}
      value={value}
      onValueChange={onChange}
      placeholder={placeholder}
      searchPlaceholder='Search tasks...'
    />
  );
};

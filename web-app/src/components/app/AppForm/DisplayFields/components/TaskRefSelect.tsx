import React, { useMemo } from "react";
import { useFormContext } from "react-hook-form";
import { Combobox } from "@/components/ui/shadcn/combobox";
import { AppFormData } from "../../index";

interface TaskRefSelectProps {
  value: string | undefined;
  onChange: (value: string) => void;
  placeholder?: string;
}

export const TaskRefSelect: React.FC<TaskRefSelectProps> = ({
  value,
  onChange,
  placeholder = "Select task...",
}) => {
  const { watch } = useFormContext<AppFormData>();
  const tasks = watch("tasks");

  const taskItems = useMemo(() => {
    const items = (tasks || [])
      .filter((task) => task?.name)
      .map((task) => ({
        value: task.name!,
        label: task.name!,
        searchText: `${task.name} ${task.type || ""}`.toLowerCase(),
      }));

    if (value && !items.some((item) => item.value === value)) {
      items.unshift({
        value,
        label: value,
        searchText: value.toLowerCase(),
      });
    }

    return items;
  }, [tasks, value]);

  if (taskItems.length === 0) {
    return (
      <div className="flex items-center h-10 px-3 border rounded-md bg-muted">
        <span className="text-sm text-muted-foreground">No tasks defined</span>
      </div>
    );
  }

  return (
    <Combobox
      items={taskItems}
      value={value}
      onValueChange={onChange}
      placeholder={placeholder}
      searchPlaceholder="Search tasks..."
    />
  );
};

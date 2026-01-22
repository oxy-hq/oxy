import React, { useMemo } from "react";
import { useFormContext } from "react-hook-form";
import { Combobox } from "@/components/ui/shadcn/combobox";
import { Input } from "@/components/ui/shadcn/input";
import { AppFormData, TaskFormData } from "../../index";

interface TaskColumnSelectProps {
  taskName: string | undefined;
  value: string | undefined;
  onChange: (value: string) => void;
  placeholder?: string;
}

function getColumnsFromTask(task: TaskFormData | undefined): string[] {
  if (!task) return [];

  if (task.type === "semantic_query") {
    const columns: string[] = [];
    const dimensions = (task as Record<string, unknown>).dimensions;
    const measures = (task as Record<string, unknown>).measures;

    if (Array.isArray(dimensions)) {
      columns.push(
        ...dimensions.filter((d): d is string => typeof d === "string"),
      );
    }
    if (Array.isArray(measures)) {
      columns.push(
        ...measures.filter((m): m is string => typeof m === "string"),
      );
    }
    return columns;
  }
  return [];
}

export const TaskColumnSelect: React.FC<TaskColumnSelectProps> = ({
  taskName,
  value,
  onChange,
  placeholder = "Column name",
}) => {
  const { watch } = useFormContext<AppFormData>();
  const tasks = watch("tasks");

  const columns = useMemo(() => {
    if (!taskName) return [];
    const taskList = tasks || [];
    const task = taskList.find((t) => t?.name === taskName);
    return getColumnsFromTask(task);
  }, [tasks, taskName]);

  const columnItems = columns.map((col) => ({
    value: col.replaceAll(".", "__"),
    label: col.replaceAll(".", "__"),
    searchText: col.toLowerCase(),
  }));

  // If no columns can be inferred, fall back to text input
  if (columnItems.length === 0) {
    return (
      <Input
        value={value || ""}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
      />
    );
  }

  return (
    <Combobox
      items={columnItems}
      value={value}
      onValueChange={onChange}
      placeholder={placeholder}
      searchPlaceholder="Search columns..."
    />
  );
};

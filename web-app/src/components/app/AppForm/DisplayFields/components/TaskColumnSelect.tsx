import React, { useMemo } from "react";
import { useFormContext } from "react-hook-form";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/shadcn/select";
import { Input } from "@/components/ui/shadcn/input";
import { AppFormData } from "../../index";

interface TaskColumnSelectProps {
  taskName: string | undefined;
  value: string | undefined;
  onChange: (value: string) => void;
  placeholder?: string;
}

const safeStringify = (val: unknown): string => {
  try {
    return JSON.stringify(val ?? null);
  } catch {
    return "null";
  }
};

const safeParse = (json: string): unknown => {
  try {
    return JSON.parse(json);
  } catch {
    return null;
  }
};

export const TaskColumnSelect: React.FC<TaskColumnSelectProps> = ({
  taskName,
  value,
  onChange,
  placeholder = "Column name",
}) => {
  const { watch } = useFormContext<AppFormData>();
  const tasks = watch("tasks");

  // Find the relevant task and create stable string keys for memoization
  const task = (tasks || []).find((t) => t?.name === taskName);
  const taskType = task?.type;
  const rawDimensions = (task as Record<string, unknown> | undefined)
    ?.dimensions;
  const rawMeasures = (task as Record<string, unknown> | undefined)?.measures;

  // Memoize JSON serialization to avoid stringify on every render
  const dimensionsJson = safeStringify(rawDimensions);
  const measuresJson = safeStringify(rawMeasures);

  const columns = useMemo(() => {
    if (taskType !== "semantic_query") return [];

    const cols: string[] = [];
    const dimensions = safeParse(dimensionsJson);
    const measures = safeParse(measuresJson);

    if (Array.isArray(dimensions)) {
      cols.push(
        ...dimensions.filter(
          (d: unknown): d is string => typeof d === "string" && d.length > 0,
        ),
      );
    }
    if (Array.isArray(measures)) {
      cols.push(
        ...measures.filter(
          (m: unknown): m is string => typeof m === "string" && m.length > 0,
        ),
      );
    }
    return [...new Set(cols)];
  }, [taskType, dimensionsJson, measuresJson]);

  const columnItems = useMemo(() => {
    const items = columns.map((col) => ({
      value: col.replaceAll(".", "__"),
      label: col.replaceAll(".", "__"),
    }));

    // If current value exists but not in columns, add it to items
    if (value && !items.some((item) => item.value === value)) {
      items.unshift({
        value,
        label: value,
      });
    }

    return items;
  }, [columns, value]);

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
    <Select value={value} onValueChange={onChange}>
      <SelectTrigger className="w-full">
        <SelectValue placeholder={placeholder} />
      </SelectTrigger>
      <SelectContent>
        {columnItems.map((item) => (
          <SelectItem key={item.value} value={item.value}>
            {item.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
};

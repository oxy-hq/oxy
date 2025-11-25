import React from "react";
import { useFormContext } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { AppFormData } from "../index";

interface TableDisplayFieldsProps {
  index: number;
}

export const TableDisplayFields: React.FC<TableDisplayFieldsProps> = ({
  index,
}) => {
  const { register } = useFormContext<AppFormData>();

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor={`display.${index}.data`}>Data Source *</Label>
        <Input
          id={`display.${index}.data`}
          placeholder="Task name (e.g., task_1)"
          {...register(`display.${index}.data`, {
            required: "Data source is required",
          })}
        />
        <p className="text-sm text-muted-foreground">
          Reference output from a task by task name
        </p>
      </div>
      <div className="space-y-2">
        <Label htmlFor={`display.${index}.title`}>Title</Label>
        <Input
          id={`display.${index}.title`}
          placeholder="Table title"
          {...register(`display.${index}.title`)}
        />
      </div>
    </div>
  );
};

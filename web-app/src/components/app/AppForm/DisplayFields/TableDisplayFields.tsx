import React from "react";
import { useFormContext, Controller } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { AppFormData } from "../index";
import { TaskRefSelect } from "./components";

interface TableDisplayFieldsProps {
  index: number;
}

export const TableDisplayFields: React.FC<TableDisplayFieldsProps> = ({
  index,
}) => {
  const { register, control } = useFormContext<AppFormData>();

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor={`display.${index}.data`}>Data Source *</Label>
        <Controller
          control={control}
          name={`display.${index}.data`}
          rules={{ required: "Data source is required" }}
          render={({ field }) => (
            <TaskRefSelect
              value={field.value as string | undefined}
              onChange={field.onChange}
              placeholder="Select task..."
            />
          )}
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

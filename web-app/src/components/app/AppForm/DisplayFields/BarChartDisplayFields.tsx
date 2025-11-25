import React from "react";
import { useFormContext } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { AppFormData } from "../index";

interface BarChartDisplayFieldsProps {
  index: number;
}

export const BarChartDisplayFields: React.FC<BarChartDisplayFieldsProps> = ({
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
          placeholder="Chart title"
          {...register(`display.${index}.title`)}
        />
      </div>
      <div className="grid grid-cols-2 gap-4">
        <div className="space-y-2">
          <Label htmlFor={`display.${index}.x`}>X Axis *</Label>
          <Input
            id={`display.${index}.x`}
            placeholder="Column name"
            {...register(`display.${index}.x`, {
              required: "X axis is required",
            })}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor={`display.${index}.y`}>Y Axis *</Label>
          <Input
            id={`display.${index}.y`}
            placeholder="Column name"
            {...register(`display.${index}.y`, {
              required: "Y axis is required",
            })}
          />
        </div>
      </div>
      <div className="grid grid-cols-2 gap-4">
        <div className="space-y-2">
          <Label htmlFor={`display.${index}.x_axis_label`}>X Axis Label</Label>
          <Input
            id={`display.${index}.x_axis_label`}
            placeholder="Optional label"
            {...register(`display.${index}.x_axis_label`)}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor={`display.${index}.y_axis_label`}>Y Axis Label</Label>
          <Input
            id={`display.${index}.y_axis_label`}
            placeholder="Optional label"
            {...register(`display.${index}.y_axis_label`)}
          />
        </div>
      </div>
      <div className="space-y-2">
        <Label htmlFor={`display.${index}.series`}>Series</Label>
        <Input
          id={`display.${index}.series`}
          placeholder="Optional series column"
          {...register(`display.${index}.series`)}
        />
      </div>
    </div>
  );
};

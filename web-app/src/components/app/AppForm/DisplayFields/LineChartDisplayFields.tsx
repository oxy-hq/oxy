import React from "react";
import { useFormContext, Controller } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { AppFormData } from "../index";
import { TaskRefSelect, TaskColumnSelect } from "./components";

interface LineChartDisplayFieldsProps {
  index: number;
}

export const LineChartDisplayFields: React.FC<LineChartDisplayFieldsProps> = ({
  index,
}) => {
  const { register, watch, control } = useFormContext<AppFormData>();
  const dataSource = watch(`display.${index}.data`) as string | undefined;

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
          placeholder="Chart title"
          {...register(`display.${index}.title`)}
        />
      </div>
      <div className="grid grid-cols-2 gap-4">
        <div className="space-y-2">
          <Label htmlFor={`display.${index}.x`}>X Axis *</Label>
          <Controller
            control={control}
            name={`display.${index}.x`}
            rules={{ required: "X axis is required" }}
            render={({ field }) => (
              <TaskColumnSelect
                taskName={dataSource}
                value={field.value as string | undefined}
                onChange={field.onChange}
                placeholder="Column name"
              />
            )}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor={`display.${index}.y`}>Y Axis *</Label>
          <Controller
            control={control}
            name={`display.${index}.y`}
            rules={{ required: "Y axis is required" }}
            render={({ field }) => (
              <TaskColumnSelect
                taskName={dataSource}
                value={field.value as string | undefined}
                onChange={field.onChange}
                placeholder="Column name"
              />
            )}
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
        <Controller
          control={control}
          name={`display.${index}.series`}
          render={({ field }) => (
            <TaskColumnSelect
              taskName={dataSource}
              value={field.value as string | undefined}
              onChange={field.onChange}
              placeholder="Optional series column"
            />
          )}
        />
      </div>
    </div>
  );
};

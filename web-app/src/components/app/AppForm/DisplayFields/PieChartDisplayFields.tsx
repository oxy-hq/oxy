import type React from "react";
import { Controller, useFormContext } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import type { AppFormData } from "../index";
import { TaskColumnSelect, TaskRefSelect } from "./components";

interface PieChartDisplayFieldsProps {
  index: number;
}

export const PieChartDisplayFields: React.FC<PieChartDisplayFieldsProps> = ({ index }) => {
  const { register, watch, control } = useFormContext<AppFormData>();
  const dataSource = watch(`display.${index}.data`) as string | undefined;

  return (
    <div className='space-y-4'>
      <div className='space-y-2'>
        <Label htmlFor={`display.${index}.data`}>Data Source *</Label>
        <Controller
          control={control}
          name={`display.${index}.data`}
          rules={{ required: "Data source is required" }}
          render={({ field }) => (
            <TaskRefSelect
              value={field.value as string | undefined}
              onChange={field.onChange}
              placeholder='Select task...'
            />
          )}
        />
        <p className='text-muted-foreground text-sm'>Reference output from a task by task name</p>
      </div>
      <div className='space-y-2'>
        <Label htmlFor={`display.${index}.title`}>Title</Label>
        <Input
          id={`display.${index}.title`}
          placeholder='Chart title'
          {...register(`display.${index}.title`)}
        />
      </div>
      <div className='grid grid-cols-2 gap-4'>
        <div className='space-y-2'>
          <Label htmlFor={`display.${index}.name`}>Name Column *</Label>
          <Controller
            control={control}
            name={`display.${index}.name`}
            rules={{ required: "Name column is required" }}
            render={({ field }) => (
              <TaskColumnSelect
                taskName={dataSource}
                value={field.value as string | undefined}
                onChange={field.onChange}
                placeholder='Column for labels'
              />
            )}
          />
        </div>
        <div className='space-y-2'>
          <Label htmlFor={`display.${index}.value`}>Value Column *</Label>
          <Controller
            control={control}
            name={`display.${index}.value`}
            rules={{ required: "Value column is required" }}
            render={({ field }) => (
              <TaskColumnSelect
                taskName={dataSource}
                value={field.value as string | undefined}
                onChange={field.onChange}
                placeholder='Column for values'
              />
            )}
          />
        </div>
      </div>
    </div>
  );
};
